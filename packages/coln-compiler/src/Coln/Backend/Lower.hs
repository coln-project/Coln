-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT
{-# OPTIONS_GHC -Wno-unused-imports #-}

module Coln.Backend.Lower where

import Control.Arrow (first, second)
import Control.Monad (forM, forM_)
import Control.Monad.RWS
import Control.Monad.Reader.Class
import Control.Monad.State.Class
import Data.Aeson qualified as AE
import Data.Map.Ordered (OMap, (>|))
import Data.Map.Ordered qualified as OMap
import Data.Map.Strict qualified as Map
import Data.Set qualified as Set
import Data.Traversable (mapAccumL)
import Prettyprinter.Render.Text (hPutDoc)
import System.FilePath ((</>))
import System.IO (IOMode (..), withFile)
import Prelude hiding (lookup)

import Coln.Backend.IR qualified as I
import Coln.Common
import Coln.Core.Evaluation
import Coln.Core.Globals qualified as C
import Coln.Core.Params
import Coln.Core.Syntax qualified as S
import Coln.Core.Value qualified as V

data Shape
  = RowId TableName
  | BuiltinTy BuiltinTy
  | Tuple (Dict Shape)
  | Unit
  deriving (Show)

projShape :: Shape -> Name -> Shape
projShape sh x = case sh of
  Tuple fields -> elemAt fields x
  _ -> panic "expected a tuple shape"

storedWidth :: Shape -> Int
storedWidth = \case
  RowId _ -> 1
  BuiltinTy _ -> 1
  Tuple fields -> sum $ storedWidth <$> fields
  Unit -> 0

data Term
  = Var BId
  | Lookup TableName (Dict Term)
  | Cons (Dict Term)
  | Proj Term Name
  | Lit Literal
  deriving (Show)

data Pred
  = EltOf Term TableName (Dict Term)
  | Holds TableName (Dict Term) -- Alternatively, `EltOf (Maybe Term) ...`?
  | And (Dict Pred)
  | Equal Term Term
  | PTrue
  deriving (Show)

type CtxLen = Int

class Lower a b | a -> b where
  lower :: CtxLen -> a -> b

instance Lower V.Head Term where
  lower n (V.LocalVar (FId i)) = Var (BId (n - i - 1))
  lower _ (V.GlobalVar _ _) = panic "not fully evaluated"
  lower n (V.Lookup x ts _) = Lookup x (lower n <$> ts)

instance Lower V.Spine (Term -> Term) where
  lower n = \case
    V.Id -> \t -> t
    V.App _ _ -> panic "not fully laid out"
    V.Proj sp x -> \t -> Proj (lower n sp t) x

instance Lower V.Neutral Term where
  lower n ne = case ne.head of
    V.Lookup{} -> lower n ne.spine $ lower n ne.head
    _ -> case ne.expansion of
      V.IntoCons fields -> Cons (lower n <$> fields)
      V.NotApplicable -> lower n ne.spine $ lower n ne.head

instance Lower (V.El N) Term where
  lower :: CtxLen -> V.El N -> Term
  lower n = \case
    V.Neu ne -> lower n ne
    V.InitNeu _ -> panic "can't lower init yet"
    V.Code _ -> panic "non set-level term"
    V.Lam _ _ -> panic "non set-level term"
    V.Cons ds -> Cons (lower n <$> ds)
    V.Lit l -> Lit l

data Ty = Ty
  { shape :: Shape
  , pred :: Pred
  }
  deriving (Show)

separate :: CtxLen -> V.Ty N -> V.El N -> Ty
separate n = \case
  V.U _ -> panic "lowering non-set-level type: U"
  V.Decode ne -> case ne.description of
    Just (V.Record rt) -> \v -> do
      let go :: V.Locals -> [(Name, V.Locals -> V.Ty N)] -> [(Shape, Pred)]
          go _ [] = []
          go vs ((x, f) : rest) = do
            let a = f vs
            let v' = V.proj v x
            let t = separate n a v'
            (t.shape, t.pred) : go (V.LSnoc vs v') rest
      let (shapes, props) = unzip $ go rt.capture (toList rt.fieldTypes)
      Ty (Tuple (withHead rt.fieldTypes shapes)) (And (withHead rt.fieldTypes props))
    Nothing -> panic "lowering neutral type"
  V.InitDecode _ -> panic "can't lower init yet"
  V.Function _ -> panic "lowering non-set-level type: Function"
  V.Eq et -> \_ -> Ty Unit (Equal (lower n et.lhs) (lower n et.rhs))
  V.BuiltinTy t -> \_ -> Ty (BuiltinTy t) PTrue
  V.EltOf x ts u -> case u of
    SetU -> \v -> Ty (RowId x) (EltOf (lower n v) x (lower n <$> ts))
    PropU -> \_ -> Ty Unit (Holds x (lower n <$> ts))
    TheoryU -> \_ -> panic "element of a non-set relation"

data Generator
  = Rel [Name] [Ty] Universe
  | Fun [Name] [Ty] Ty

lowerAtFresh :: CtxLen -> V.Ty N -> Ty
lowerAtFresh n a = separate (n + 1) a (V.local (FId n) a)

lowerTele :: [S.Ty N] -> ([Ty], V.Locals)
lowerTele = go V.LNil 0
 where
  go vs _ [] = ([], vs)
  go vs n (t : ts) = do
    let a = eval vs t
    let v = V.local (FId n) a
    let (ts', vs') = go (V.LSnoc vs v) (n + 1) ts
    (separate (n + 1) a v : ts', vs')

lowerGen :: C.Generator -> Generator
lowerGen (C.Fun xs ts t) = do
  let (ts', vs) = lowerTele ts
  Fun xs ts' (lowerAtFresh (length ts) (eval vs t))
lowerGen (C.Rel xs ts u) = do
  let (ts', _) = lowerTele ts
  Rel xs ts' u

data TableInfo = TableInfo
  {tableShapes :: OMap TableName Generator}

data VarInfo = VarInfo
  { vars :: Bwd (I.ColName, I.ColType)
  , length :: Int
  , usedRoots :: Set.Set Name
  }

newtype FlatM a = FlatM {unFlatM :: RWS TableInfo (Bwd I.Prop) VarInfo a}
  deriving (Functor, Applicative, Monad, MonadState VarInfo, MonadWriter (Bwd I.Prop), MonadReader TableInfo)

runFlatM :: TableInfo -> FlatM a -> (a, Bwd (I.ColName, I.ColType), Bwd I.Prop)
runFlatM ti action = do
  let (res, s, w) = runRWS action.unFlatM ti (VarInfo BwdNil 0 Set.empty)
  (res, s.vars, w)

scalarShape :: Shape -> Maybe I.ColType
scalarShape = \case
  RowId x -> Just $ I.RowId x
  BuiltinTy t -> Just $ I.BuiltinTy t
  _ -> Nothing

fresh :: Path -> Shape -> FlatM (Trie I.Term)
fresh p = \case
  (scalarShape -> Just colTy) -> do
    ni <- get
    let i = ni.length
    put $ ni{vars = (ni.vars :> (p, colTy)), length = (i + 1)}
    pure $ Leaf $ I.Var $ FId i
  (RowId _; BuiltinTy _) -> panic "should have been covered by scalarShape"
  Tuple fields -> do
    fields' <- forM (toList fields) $ \(x, sh) -> do
      v <- fresh (p :> x) sh
      pure (x, v)
    pure $ Node $ fromList fields'
  Unit -> pure $ Node (fromList [])

freshName :: FlatM Name
freshName = do
  ni <- get
  let x = freshNameFor ni.usedRoots
  put $ ni{usedRoots = Set.insert x ni.usedRoots}
  pure x

emit :: I.Prop -> FlatM ()
emit = tell . (BwdNil :>)

retShapeOf :: TableName -> FlatM Shape
retShapeOf tn = do
  ti <- ask
  case OMap.lookup tn ti.tableShapes of
    Just (Fun _ _ ret) -> pure ret.shape
    _ -> panic "can only lookup from function"

type FlatEnv = Bwd (Trie I.Term)

class Flatten a b | a -> b where
  flatten :: FlatEnv -> a -> FlatM b

instance Flatten Term (Trie I.Term) where
  flatten e = \case
    Var i -> pure $ elemAt e i
    Lookup x ts -> do
      retSh <- retShapeOf x
      retName <- freshName
      ret <- fresh (BwdNil :> retName) retSh
      args <- mapM (flatten e) $ toList ts.values
      emit $ I.PAtom $ I.Atom x Nothing $ OMap.fromList $ (zip [0 ..] (flattenTries (args ++ [ret])))
      pure ret
    Proj t x -> do
      v <- flatten e t
      pure $ projTrie v x
    Lit l -> pure $ Leaf $ I.Lit l
    Cons ts -> Node <$> traverse (flatten e) ts

instance Flatten Pred [I.Prop] where
  flatten e = \case
    EltOf t x ts -> do
      v <- flatten e t
      vs <- traverse (flatten e) $ toList ts.values
      pure [I.PAtom $ I.Atom x (Just (unwrapLeaf v)) (OMap.fromList $ (zip [0 ..] (flattenTries vs)))]
    Holds x ts -> do
      vs <- traverse (flatten e . snd) $ toList ts
      pure [I.PAtom $ I.Atom x Nothing (OMap.fromList $ (zip [0 ..] (flattenTries vs)))]
    And ts -> do
      pss <- mapM (flatten e) ts.values
      pure $ concat pss
    Equal t0 t1 -> do
      v0 <- flatten e t0
      v1 <- flatten e t1
      pure [I.PEq c0 c1 | (c0, c1) <- zip (flattenTrie v0) (flattenTrie v1)]
    PTrue -> pure []

useRoot :: Name -> FlatM ()
useRoot x = modify $ \ni -> ni{usedRoots = Set.insert x ni.usedRoots}

validity :: [(Name, Ty)] -> FlatM (Bwd (Trie I.Term), [I.Prop])
validity = go BwdNil BwdNil
 where
  go e ps [] = pure (e, toList ps)
  go e ps ((x, ty) : tys) = do
    v <- fresh (BwdNil :> x) ty.shape
    let e' = e :> v
    useRoot x
    p <- flatten e' ty.pred
    go e' (ps ++> p) tys

flattenArg :: I.ColName -> Shape -> [(I.ColName, I.ColType)]
flattenArg p sh = case sh of
  (scalarShape -> Just colTy) -> [(p, colTy)]
  (RowId _; BuiltinTy _) -> panic "should have been covered by scalarShape"
  Tuple fields -> concat [flattenArg (p :> x) sh' | (x, sh') <- toList fields]
  Unit -> []

flattenArgs :: [(Name, Ty)] -> [(I.ColName, I.ColType)]
flattenArgs args = concat [flattenArg (BwdNil :> x) ty.shape | (x, ty) <- args]

createRule :: TableInfo -> I.RuleVariant -> FlatM ([I.Prop], [I.Prop]) -> I.Rule
createRule ti variant action = do
  let ((ante, cons), vars, props) = runFlatM ti action
  let (varNames, varTypes) = unzip $ toList vars
  I.Rule
    { I.ruleVariant = variant
    , I.varNames = fromList varNames
    , I.varTypes = fromList varTypes
    , I.antecedents = toList (props ++> ante)
    , I.consequents = cons
    }

flattenGen :: TableInfo -> TableName -> Generator -> (Maybe I.Entity, [(TableName, I.Rule)])
flattenGen ti tn = \case
  Rel xs tys u -> do
    let args = zip xs tys
    let cols = flattenArgs args
    let pkey = case u of
          SetU -> Nothing
          PropU -> Just $ Set.fromList $ fst <$> cols
          _ -> panic "generator must be of a set or smaller universe"
    let entity = I.Entity I.Table cols pkey
    let foreignKeyRule = createRule ti I.Enforced $ do
          (e, cons) <- validity args
          let vs = flattenTries $ toList e
          let ante = [I.PAtom (I.Atom tn Nothing (OMap.fromList $ zip [0 ..] vs))]
          pure (ante, cons)
    (Just entity, [(tn{path = tn.path :> "foreignKey"}, foreignKeyRule)])
  Fun xs argTys retTy -> case storedWidth retTy.shape of
    0 -> do
      let args = zip xs argTys
      let rule = createRule ti I.Monitored $ do
            (e, argPreds) <- validity args
            let ret = Node (fromList [])
            retPred <- flatten (e :> ret) retTy.pred
            pure (argPreds, retPred)
      (Nothing, [(tn, rule)])
    _ -> do
      let args = zip xs argTys
      let argCols = flattenArgs args
      let pkey = Just $ Set.fromList $ fst <$> argCols
      let retName = BwdNil :> freshNameFor xs
      let retCols = flattenArg retName retTy.shape
      let cols = argCols ++ retCols
      let entity = I.Entity I.Table cols pkey
      let foreignKeyRule = createRule ti I.Enforced $ do
            (e, argPreds) <- validity args
            ret <- fresh retName retTy.shape
            let e' = e :> ret
            retPred <- flatten (e :> ret) retTy.pred
            let vs = flattenTries $ toList e'
            let ante = [I.PAtom (I.Atom tn Nothing (OMap.fromList $ zip [0 ..] vs))]
            pure (ante, argPreds ++ retPred)
      let totalityRule = createRule ti I.Monitored $ do
            (e, argPreds) <- validity args
            let vs = flattenTries $ toList e
            let cons = [I.PAtom (I.Atom tn Nothing (OMap.fromList $ zip [0 ..] vs))]
            pure (argPreds, cons)
      ( Just entity
        ,
          [ (tn{path = tn.path :> "foreignKey"}, foreignKeyRule)
          , (tn{path = tn.path :> "total"}, totalityRule)
          ]
        )

addFlatGenerator :: TableInfo -> I.FlatRealm -> TableName -> Generator -> I.FlatRealm
addFlatGenerator ti fr x g = do
  let (me, rules) = flattenGen ti x g
  fr
    { I.entities = case me of
        Just e -> fr.entities OMap.>| (x, e)
        Nothing -> fr.entities
    , I.rules = fr.rules OMap.<>| (OMap.fromList rules)
    }

lowerRealm :: Name -> C.Realm -> I.FlatRealm
lowerRealm realmName r = go I.emptyFlatRealm $ OMap.assocs generators
 where
  generators =
    OMap.fromList $
      for (toList r.generators) $ \(path, generator) ->
        (TableName realmName path, lowerGen generator)

  tableInfo = TableInfo generators

  go fr [] = fr
  go fr ((tn, generator) : rest) =
    go (addFlatGenerator tableInfo fr tn generator) rest

writeIRFor :: C.Globals -> FilePath -> IO ()
writeIRFor ge fp = do
  forM_ (OMap.assocs ge.realms) $ \(x, r) -> do
    let fr = lowerRealm x r
    let fn = fp </> mangleToString x <> ".json"
    AE.encodeFile fn fr
    let pn = fp </> mangleToString x <> ".pretty"
    withFile pn WriteMode (\h -> hPutDoc h (dpretty fr))
