-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Backend.Lower where

import Control.Arrow (first, second)
import Control.Monad (forM_)
import Data.Aeson qualified as AE
import Data.Foldable qualified as F
import Data.Map.Ordered (OMap, (>|))
import Data.Map.Ordered qualified as OMap
import Data.Set qualified as Set
import Data.Traversable (mapAccumL)
import Prettyprinter.Render.Text (hPutDoc)
import System.FilePath ((</>))
import System.IO (IOMode (..), withFile)
import Prelude hiding (lookup)

import Coln.Backend.IR qualified as I
import Coln.Common
import Coln.Core.Evaluation
import Coln.Core.Globals
import Coln.Core.Params
import Coln.Core.Realm qualified as C
import Coln.Core.Syntax qualified as S
import Coln.Core.Value qualified as V

data Shape
  = RowId TableName
  | BuiltinTy BuiltinTy
  | Tuple (Dict Shape)
  | Unit
  deriving (Show)

data Term
  = Var BId
  | Lookup TableName (Dict Term)
  | Cons (Dict Term)
  | Proj Term Name
  | Lit Literal
  deriving (Show)

data Pred
  = EltOf Term TableName (Dict Term)
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

instance Lower V.Spine (Term -> Term) where
  lower n = \case
    V.Id -> \t -> t
    V.App _ _ -> panic "not fully laid out"
    V.Proj sp x -> \t -> Proj (lower n sp t) x

instance Lower V.Neutral Term where
  lower n ne = case ne.expansion of
    V.IntoCons fields -> Cons (lower n <$> fields)
    V.NotApplicable -> lower n ne.spine $ lower n ne.head

instance Lower (V.El N) Term where
  lower :: CtxLen -> V.El N -> Term
  lower n = \case
    V.Neu ne -> lower n ne
    V.Code _ -> panic "non set-level term"
    V.Lam _ _ -> panic "non set-level term"
    V.Cons ds -> Cons (lower n <$> ds)
    V.Lit l -> Lit l
    V.Lookup x ts -> Lookup x (lower n <$> ts)

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
  V.Function _ -> panic "lowering non-set-level type: Function"
  V.Eq et -> \_ -> Ty Unit (Equal (lower n et.lhs) (lower n et.rhs))
  V.BuiltinTy t -> \_ -> Ty (BuiltinTy t) PTrue
  V.EltOf x ts -> \v -> Ty (RowId x) (EltOf (lower n v) x (lower n <$> ts))

data Generator
  = Rel [Name] [Ty]
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
lowerGen (C.Rel xs ts) = do
  let (ts', _) = lowerTele ts
  Rel xs ts'

type EnvTerm = Trie I.Term

noTerms :: EnvTerm
noTerms = Node $ fromList []

data LocalCtx = LocalCtx
  { localLen :: CtxLen
  , totalLen :: CtxLen
  , localNames :: Bwd I.ColName
  , localTys :: Bwd I.ColType
  , conditions :: Bwd I.Prop
  }

data RuleFragment = RuleFragment
  { ruleCtx :: LocalCtx
  , heads :: [(I.ColName, I.Prop)]
  }

data DisaggState = DisaggState
  { funShapes :: OMap TableName Shape
  , oldLen :: CtxLen
  , oldNames :: Bwd Name
  , oldTys :: Bwd Ty
  , oldEnv :: Bwd EnvTerm
  , newLen :: CtxLen
  , newNames :: Bwd I.ColName
  , newTys :: Bwd I.ColType
  , frags :: Bwd RuleFragment
  }

data PredState = PredState
  { parent :: DisaggState
  , localCtx :: LocalCtx
  }

steal :: Bwd a -> CtxLen -> Bwd a -> Bwd a
steal base 0 _ = base
steal base n (xs :> x) = steal base (n - 1) xs :> x
steal _ _ _ = panic "not enough local variables"

renumberTerm :: (Int -> Int) -> I.Term -> I.Term
renumberTerm f (I.Var (FId i)) = I.Var . FId $ f i
renumberTerm _ x = x

renumberProp :: (Int -> Int) -> I.Prop -> I.Prop
renumberProp f (I.PAtom atom) =
  I.PAtom $
    atom
      { I.rowId = fmap (renumberTerm f) atom.rowId
      , I.values = fmap (renumberTerm f) atom.values
      }
renumberProp f (I.PEq lhs rhs) =
  I.PEq
    (renumberTerm f lhs)
    (renumberTerm f rhs)

-- the global variables of the second must be a prefix of the global variables
-- of the first
mergeFrag :: RuleFragment -> RuleFragment -> RuleFragment
mergeFrag base add = do
  let basec = base.ruleCtx
  let addc = add.ruleCtx
  let renum n = if n >= addc.totalLen - addc.localLen then n - addc.totalLen + addc.localLen + basec.totalLen else n
  RuleFragment
    { ruleCtx =
        LocalCtx
          { localLen = basec.localLen + addc.localLen
          , totalLen = basec.totalLen + addc.localLen
          , localNames = steal basec.localNames addc.localLen addc.localNames
          , localTys = steal basec.localTys addc.localLen addc.localTys
          , conditions = basec.conditions <> fmap (renumberProp renum) addc.conditions
          }
    , heads = base.heads ++ fmap (second $ renumberProp renum) add.heads
    }

pushNew :: DisaggState -> (I.ColName, I.ColType) -> (DisaggState, EnvTerm)
pushNew ds (cn, ct) = do
  let et = Leaf $ I.Var $ FId $ ds.newLen
  let ds' =
        ds
          { newLen = ds.newLen + 1
          , newNames = ds.newNames :> cn
          , newTys = ds.newTys :> ct
          }
  (ds', et)

pushShape :: DisaggState -> (I.ColName, Shape) -> (DisaggState, EnvTerm)
pushShape ds = uncurry $ \x -> \case
  RowId y -> pushNew ds (x, I.RowId y)
  BuiltinTy bt -> pushNew ds (x, I.BuiltinTy bt)
  Tuple d -> second (Node . withHead d) . mapAccumL pushShape ds . fmap (first (x :>)) $ toList d
  Unit -> (ds, noTerms)

pushOld :: DisaggState -> (Name, Ty, EnvTerm) -> DisaggState
pushOld ds (x, ty, et) = ds{oldLen = ds.oldLen + 1, oldNames = ds.oldNames :> x, oldTys = ds.oldTys :> ty, oldEnv = ds.oldEnv :> et}

openPred :: DisaggState -> PredState
openPred ds =
  PredState ds $
    LocalCtx
      { localLen = 0
      , totalLen = ds.newLen
      , localNames = ds.newNames
      , localTys = ds.newTys
      , conditions = BwdNil
      }

pushFrag :: PredState -> I.ColName -> [I.Prop] -> DisaggState
pushFrag ps x h = ps.parent{frags = ps.parent.frags :> RuleFragment ps.localCtx (fmap (\y -> (x, y)) h)}

pushLocal :: PredState -> (I.ColName, I.ColType) -> (PredState, EnvTerm)
pushLocal ps (cn, ct) = do
  let et = Leaf $ I.Var $ FId $ ps.localCtx.totalLen
  let ctx' =
        ps.localCtx
          { localLen = ps.localCtx.localLen + 1
          , totalLen = ps.localCtx.totalLen + 1
          , localNames = ps.localCtx.localNames :> cn
          , localTys = ps.localCtx.localTys :> ct
          }
  (ps{localCtx = ctx'}, et)

pushVars :: PredState -> (I.ColName, Shape) -> (PredState, EnvTerm)
pushVars ps = uncurry $ \x -> \case
  RowId tn -> pushLocal ps (x, I.RowId tn)
  BuiltinTy bt -> pushLocal ps (x, I.BuiltinTy bt)
  Tuple d -> second (Node . withHead d) . mapAccumL pushVars ps . fmap (first (x :>)) $ toList d
  Unit -> (ps, noTerms)

pushTerm' :: PredState -> (I.ColName, Term) -> (PredState, Trie I.Term)
pushTerm' ps = uncurry $ \x -> \case
  Var b -> (ps, elemAt ps.parent.oldEnv b)
  Lookup tn d -> case OMap.lookup tn ps.parent.funShapes of
    Nothing -> panic "unknown function"
    Just s -> do
      let (ps', ts) = pushVars ps (x, s)
      let ps'' = pushCond ps' x tn d ts
      (ps'', ts)
  Cons d -> second (Node . withHead d) . mapAccumL pushTerm' ps . fmap (first (x :>)) $ toList d
  Proj y f -> do
    let (ps', ts) = pushTerm' ps (x, y)
    case ts of
      Leaf _ -> panic "projection of non-record value"
      Node d -> case lookup d f of
        Nothing -> panic "nonexistent field"
        Just z -> (ps', z)
  Lit l -> (ps, Leaf $ I.Lit l)

pushTerm :: PredState -> (I.ColName, Term) -> (PredState, [I.Term])
pushTerm ps a = second F.toList $ pushTerm' ps a

pushCond :: PredState -> I.ColName -> TableName -> Dict Term -> Trie I.Term -> PredState
pushCond ps x tn d ts' = do
  let (ps', ts) = mapAccumL pushTerm ps . fmap (first (x :>)) $ toList d
  let c = I.PAtom . I.Atom tn Nothing . OMap.fromList . zip [0 ..] $ foldr (++) (F.toList ts') ts
  ps'{localCtx = ps'.localCtx{conditions = ps'.localCtx.conditions :> c}}

-- XXX actual state monad?
pushPred :: DisaggState -> (I.ColName, Pred) -> DisaggState
pushPred ds = uncurry $ \x -> \case
  EltOf t n ts -> do
    let ps1 = openPred ds
    let (ps2, elts) = pushTerm' ps1 (x, t)
    let elt = case elts of Leaf x -> x; _ -> panic "EltOf lhs was not an entity"
    let (ps3, fields) = mapAccumL pushTerm ps2 . fmap (first (x :>)) $ toList ts
    let fields' = OMap.fromList . zip [0 ..] $ concat fields
    pushFrag ps3 x [I.PAtom $ I.Atom n (Just elt) fields']
  And d -> foldl' pushPred ds . fmap (first (x :>)) $ toList d
  Equal lhs rhs -> do
    let ps = openPred ds
    let (ps', lhs') = pushTerm ps (x :> "lhs", lhs)
    let (ps'', rhs') = pushTerm ps' (x :> "rhs", rhs)
    pushFrag ps'' x $ zipWith I.PEq lhs' rhs'
  PTrue -> ds

pushTy :: DisaggState -> (Name, Ty) -> DisaggState
pushTy ds (x, ty) = do
  let (ds', et) = pushShape ds (BwdNil :> x, ty.shape)
  let ds'' = pushOld ds' (x, ty, et)
  pushPred ds'' (BwdNil :> x, ty.pred)

disaggregateTele :: OMap TableName Shape -> [Name] -> [Ty] -> DisaggState
disaggregateTele fs xs tys = do
  let ds =
        DisaggState
          { funShapes = fs
          , oldLen = 0
          , oldNames = BwdNil
          , oldTys = BwdNil
          , oldEnv = BwdNil
          , newLen = 0
          , newNames = BwdNil
          , newTys = BwdNil
          , frags = BwdNil
          }
  foldl' pushTy ds $ zip xs tys

mergeFrags :: DisaggState -> RuleFragment
mergeFrags ds = do
  let base =
        RuleFragment
          { ruleCtx =
              LocalCtx
                { localLen = 0
                , totalLen = ds.newLen
                , localNames = ds.newNames
                , localTys = ds.newTys
                , conditions = BwdNil
                }
          , heads = []
          }
  foldl' mergeFrag base $ toList ds.frags

disaggregateGen :: OMap TableName Shape -> TableName -> Generator -> I.FlatRealm -> I.FlatRealm
disaggregateGen fs tn (Rel xs ts) fr = do
  let ds = disaggregateTele fs xs ts
  let rf = mergeFrags ds
  let foreignKey =
        I.Rule
          { I.ruleVariant = I.Enforced
          , I.varNames = rf.ruleCtx.localNames
          , I.varTypes = rf.ruleCtx.localTys
          , I.antecedents = (I.PAtom $ I.Atom tn Nothing $ OMap.fromList $ map (\n -> (n, I.Var $ FId n)) [0 .. ds.newLen - 1]) : toList rf.ruleCtx.conditions
          , I.consequents = fmap snd rf.heads
          }
  let table =
        I.Entity
          { I.entityVariant = I.Table
          , I.columns = zip (toList ds.newNames) (toList ds.newTys)
          , primaryKey = Nothing
          }
  fr
    { I.entities = fr.entities >| (tn, table)
    , I.rules = fr.rules >| (tn{path = tn.path :> "foreignKey"}, foreignKey)
    }
disaggregateGen fs tn (Fun xs ts t) fr = do
  let ds = disaggregateTele fs xs ts
  let rf = mergeFrags ds
  let totality =
        I.Rule
          { I.ruleVariant = I.Monitored -- XXX do Enforced when appropriate
          , I.varNames = rf.ruleCtx.localNames
          , I.varTypes = rf.ruleCtx.localTys
          , I.antecedents = toList rf.ruleCtx.conditions ++ fmap snd rf.heads
          , I.consequents = [I.PAtom $ I.Atom tn Nothing $ OMap.fromList $ map (\n -> (n, I.Var $ FId n)) [0 .. ds.newLen - 1]]
          }
  let x = freshNameFor xs
  let ds' = pushTy ds (x, t)
  let rf' = mergeFrags ds'
  let foreignKey =
        I.Rule
          { I.ruleVariant = I.Enforced
          , I.varNames = rf'.ruleCtx.localNames
          , I.varTypes = rf'.ruleCtx.localTys
          , I.antecedents = (I.PAtom $ I.Atom tn Nothing $ OMap.fromList $ map (\n -> (n, I.Var $ FId n)) [0 .. ds'.newLen - 1]) : toList rf'.ruleCtx.conditions
          , I.consequents = fmap snd rf'.heads
          }
  let table =
        I.Entity
          { I.entityVariant = I.Table
          , I.columns = zip (toList ds'.newNames) (toList ds'.newTys)
          , I.primaryKey = Just . Set.fromList $ toList ds.newNames
          }
  fr
    { I.entities = fr.entities >| (tn, table)
    , I.rules = fr.rules >| (tn{path = tn.path :> "foreignKey"}, foreignKey) >| (tn{path = tn.path :> "total"}, totality)
    }

lowerRealm :: Name -> C.Realm -> I.FlatRealm
lowerRealm realmName r = go OMap.empty I.emptyFlatRealm (toList r.generators)
 where
  go _ fr [] = fr
  go fs fr ((xs, g) : rest) = do
    let tn = TableName realmName xs
    let lg = lowerGen g
    let fr' = disaggregateGen fs tn lg fr
    let fs' = case lg of
          Fun _ _ t -> fs >| (tn, t.shape)
          _ -> fs
    go fs' fr' rest

writeIRFor :: Globals -> FilePath -> IO ()
writeIRFor ge fp = do
  forM_ (OMap.assocs ge.realms) $ \(x, r) -> do
    let fr = lowerRealm x r
    let fn = fp </> mangleToString x <> ".json"
    AE.encodeFile fn fr
    let pn = fp </> mangleToString x <> ".pretty"
    withFile pn WriteMode (\h -> hPutDoc h (dpretty fr))
