-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Backend.Lower where

import Control.Arrow (first, second)
import Control.Monad (forM_)
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
  lower n (V.Lookup x ts _) = Lookup x (lower n <$> ts)

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
  V.EltOf x ts _ -> \v -> Ty (RowId x) (EltOf (lower n v) x (lower n <$> ts))

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

newtype ColumnBinding = ColumnBinding
  { columnIndex :: Int
  }
  deriving (Show)

newtype RuleLocalBinding = RuleLocalBinding Int
  deriving (Eq, Ord, Show)

data RuleTerm
  = ColumnTerm ColumnBinding
  | RuleLocalTerm RuleLocalBinding
  | LitTerm Literal

data RuleProp
  = RuleAtom TableName (Maybe RuleTerm) [RuleTerm]
  | RuleEq RuleTerm RuleTerm

data RuleValue
  = RuleScalar RuleTerm
  | RuleRecord (Dict RuleValue)
  | RuleErased

flattenRuleValue :: RuleValue -> [RuleTerm]
flattenRuleValue = \case
  RuleScalar term -> [term]
  RuleRecord fields -> concatMap flattenRuleValue fields
  RuleErased -> []

data RuleFragment = RuleFragment
  { ruleLocalBindings :: Bwd (RuleLocalBinding, I.ColName, I.ColType)
  , lookupConditions :: Bwd RuleProp
  , typePredicates :: Bwd RuleProp
  }

instance Semigroup RuleFragment where
  a <> b =
    RuleFragment
      { ruleLocalBindings = a.ruleLocalBindings <> b.ruleLocalBindings
      , lookupConditions = a.lookupConditions <> b.lookupConditions
      , typePredicates = a.typePredicates <> b.typePredicates
      }

instance Monoid RuleFragment where
  mempty =
    RuleFragment
      { ruleLocalBindings = BwdNil
      , lookupConditions = BwdNil
      , typePredicates = BwdNil
      }

data DisaggState = DisaggState
  { generators :: OMap TableName Generator
  , oldLen :: CtxLen
  , oldNames :: Bwd Name
  , oldTys :: Bwd Ty
  , oldEnv :: Bwd RuleValue
  , newColumns :: Bwd (ColumnBinding, I.ColName, I.ColType)
  , nextRuleLocal :: Int
  , frags :: Bwd RuleFragment
  }

data PredState = PredState
  { parent :: DisaggState
  , fragment :: RuleFragment
  }

nextColumnIndex :: Bwd (ColumnBinding, I.ColName, I.ColType) -> Int
nextColumnIndex BwdNil = 0
nextColumnIndex (_ :> (binding, _, _)) = binding.columnIndex + 1

pushNew :: DisaggState -> (I.ColName, I.ColType) -> (DisaggState, RuleValue)
pushNew ds (cn, ct) = do
  let binding = ColumnBinding $ nextColumnIndex ds.newColumns
  let ds' =
        ds
          { newColumns = ds.newColumns :> (binding, cn, ct)
          }
  (ds', RuleScalar $ ColumnTerm binding)

pushShape :: DisaggState -> (I.ColName, Shape) -> (DisaggState, RuleValue)
pushShape ds = uncurry $ \x -> \case
  RowId y -> pushNew ds (x, I.RowId y)
  BuiltinTy bt -> pushNew ds (x, I.BuiltinTy bt)
  Tuple d -> second (RuleRecord . withHead d) . mapAccumL pushShape ds . fmap (first (x :>)) $ toList d
  Unit -> (ds, RuleErased)

pushOld :: DisaggState -> (Name, Ty, RuleValue) -> DisaggState
pushOld ds (x, ty, et) = ds{oldLen = ds.oldLen + 1, oldNames = ds.oldNames :> x, oldTys = ds.oldTys :> ty, oldEnv = ds.oldEnv :> et}

openPred :: DisaggState -> PredState
openPred ds = PredState ds mempty

pushFrag :: PredState -> [RuleProp] -> DisaggState
pushFrag ps predicates = do
  let fragment =
        ps.fragment
          { typePredicates = ps.fragment.typePredicates <> fromList predicates
          }
  ps.parent{frags = ps.parent.frags :> fragment}

pushRuleLocal :: PredState -> (I.ColName, I.ColType) -> (PredState, RuleValue)
pushRuleLocal ps (cn, ct) = do
  let binding = RuleLocalBinding ps.parent.nextRuleLocal
  let parent' = ps.parent{nextRuleLocal = ps.parent.nextRuleLocal + 1}
  let fragment' =
        ps.fragment
          { ruleLocalBindings = ps.fragment.ruleLocalBindings :> (binding, cn, ct)
          }
  (ps{parent = parent', fragment = fragment'}, RuleScalar $ RuleLocalTerm binding)

pushVars :: PredState -> (I.ColName, Shape) -> (PredState, RuleValue)
pushVars ps = uncurry $ \x -> \case
  RowId tn -> pushRuleLocal ps (x, I.RowId tn)
  BuiltinTy bt -> pushRuleLocal ps (x, I.BuiltinTy bt)
  Tuple d -> second (RuleRecord . withHead d) . mapAccumL pushVars ps . fmap (first (x :>)) $ toList d
  Unit -> (ps, RuleErased)

pushTerm' :: PredState -> (I.ColName, Term) -> (PredState, RuleValue)
pushTerm' ps = uncurry $ \x -> \case
  Var b -> (ps, elemAt ps.parent.oldEnv b)
  Lookup tn d -> case OMap.lookup tn ps.parent.generators of
    Nothing -> panic "unknown function"
    Just (Rel _ _ _) -> panic "looked up a relation"
    Just (Fun _ _ t) -> do
      let (ps', ts) = pushVars ps (x, t.shape)
      let ps'' = pushCond ps' x tn d ts
      (ps'', ts)
  Cons d -> second (RuleRecord . withHead d) . mapAccumL pushTerm' ps . fmap (first (x :>)) $ toList d
  Proj y f -> do
    let (ps', ts) = pushTerm' ps (x, y)
    case ts of
      RuleScalar _ -> panic "projection of non-record value"
      RuleRecord d -> case lookup d f of
        Nothing -> panic "nonexistent field"
        Just z -> (ps', z)
      RuleErased -> panic "projection of erased value"
  Lit l -> (ps, RuleScalar $ LitTerm l)

pushTerm :: PredState -> (I.ColName, Term) -> (PredState, [RuleTerm])
pushTerm ps a = second flattenRuleValue $ pushTerm' ps a

pushCond :: PredState -> I.ColName -> TableName -> Dict Term -> RuleValue -> PredState
pushCond ps x tn d ts' = do
  let (ps', ts) = mapAccumL pushTerm ps . fmap (first (x :>)) $ toList d
  let c = RuleAtom tn Nothing $ foldr (++) (flattenRuleValue ts') ts
  ps'{fragment = ps'.fragment{lookupConditions = ps'.fragment.lookupConditions :> c}}

-- XXX actual state monad?
pushPred :: DisaggState -> (I.ColName, Pred) -> DisaggState
pushPred ds = uncurry $ \x -> \case
  EltOf t n ts -> do
    let ps1 = openPred ds
    let (ps2, elts) = pushTerm' ps1 (x, t)
    let elt = case elts of RuleScalar term -> term; _ -> panic "EltOf lhs was not an entity"
    let (ps3, fields) = mapAccumL pushTerm ps2 . fmap (first (x :>)) $ toList ts
    pushFrag ps3 [RuleAtom n (Just elt) $ concat fields]
  And d -> foldl' pushPred ds . fmap (first (x :>)) $ toList d
  Equal lhs rhs -> do
    let ps = openPred ds
    let (ps', lhs') = pushTerm ps (x :> "lhs", lhs)
    let (ps'', rhs') = pushTerm ps' (x :> "rhs", rhs)
    pushFrag ps'' $ zipWith RuleEq lhs' rhs'
  PTrue -> ds

pushTy :: DisaggState -> (Name, Ty) -> DisaggState
pushTy ds (x, ty) = do
  let (ds', et) = pushShape ds (BwdNil :> x, ty.shape)
  let ds'' = pushOld ds' (x, ty, et)
  pushPred ds'' (BwdNil :> x, ty.pred)

disaggregateTele :: OMap TableName Generator -> [Name] -> [Ty] -> DisaggState
disaggregateTele gs xs tys = do
  let ds =
        DisaggState
          { generators = gs
          , oldLen = 0
          , oldNames = BwdNil
          , oldTys = BwdNil
          , oldEnv = BwdNil
          , newColumns = BwdNil
          , nextRuleLocal = 0
          , frags = BwdNil
          }
  foldl' pushTy ds $ zip xs tys

mergeFrags :: DisaggState -> RuleFragment
mergeFrags ds = foldl' (<>) mempty ds.frags

-- When the number of columns is finally known, we can assign `FIds`
-- to the rule-local variables
buildRule :: Bwd (ColumnBinding, I.ColName, I.ColType) -> I.RuleVariant -> RuleFragment -> [RuleProp] -> [RuleProp] -> I.Rule
buildRule columns variant fragment antecedents consequents =
  I.Rule
    { I.ruleVariant = variant
    , I.varNames = fmap (\(_, name, _) -> name) columns <> fmap (\(_, name, _) -> name) fragment.ruleLocalBindings
    , I.varTypes = fmap (\(_, _, ty) -> ty) columns <> fmap (\(_, _, ty) -> ty) fragment.ruleLocalBindings
    , I.antecedents = toIRProp <$> antecedents
    , I.consequents = toIRProp <$> consequents
    }
 where
  ruleLocalIds =
    Map.fromList $
      for (zip [nextColumnIndex columns ..] $ toList fragment.ruleLocalBindings) $ \(index, (binding, _, _)) ->
        (binding, FId index)

  toIRTerm = \case
    ColumnTerm binding -> I.Var $ FId binding.columnIndex
    RuleLocalTerm binding -> I.Var $ ruleLocalIds Map.! binding
    LitTerm literal -> I.Lit literal

  toIRProp = \case
    RuleAtom entity rowId values ->
      I.PAtom $
        I.Atom
          entity
          (toIRTerm <$> rowId)
          (OMap.fromList $ zip [0 ..] $ toIRTerm <$> values)
    RuleEq lhs rhs -> I.PEq (toIRTerm lhs) (toIRTerm rhs)

tableAtom :: TableName -> Bwd (ColumnBinding, I.ColName, I.ColType) -> RuleProp
tableAtom name columns =
  RuleAtom name Nothing $
    for (toList columns) $ \(binding, _, _) ->
      ColumnTerm binding

disaggregateGen :: OMap TableName Generator -> TableName -> Generator -> I.FlatRealm -> I.FlatRealm
disaggregateGen gs tn (Rel xs ts _) fr = do
  let ds = disaggregateTele gs xs ts
  let rf = mergeFrags ds
  let columns = ds.newColumns
  let foreignKey =
        buildRule
          columns
          I.Enforced
          rf
          (tableAtom tn columns : toList rf.lookupConditions)
          (toList rf.typePredicates)
  let table =
        I.Entity
          { I.entityVariant = I.Table
          , I.columns = for (toList columns) $ \(_, name, ty) -> (name, ty)
          , primaryKey = Nothing
          }
  fr
    { I.entities = fr.entities >| (tn, table)
    , I.rules = fr.rules >| (tn{path = tn.path :> "foreignKey"}, foreignKey)
    }
disaggregateGen gs tn (Fun xs ts t) fr = do
  let ds = disaggregateTele gs xs ts
  let rf = mergeFrags ds
  let parameterColumns = ds.newColumns
  let totality =
        buildRule
          parameterColumns
          I.Monitored -- XXX do Enforced when appropriate
          rf
          (toList rf.lookupConditions ++ toList rf.typePredicates)
          [tableAtom tn parameterColumns]
  let x = freshNameFor xs
  let ds' = pushTy ds (x, t)
  let rf' = mergeFrags ds'
  let columns = ds'.newColumns
  let foreignKey =
        buildRule
          columns
          I.Enforced
          rf'
          (tableAtom tn columns : toList rf'.lookupConditions)
          (toList rf'.typePredicates)
  let table =
        I.Entity
          { I.entityVariant = I.Table
          , I.columns = for (toList columns) $ \(_, name, ty) -> (name, ty)
          , I.primaryKey = Just . Set.fromList $ for (toList parameterColumns) $ \(_, name, _) -> name
          }
  fr
    { I.entities = fr.entities >| (tn, table)
    , I.rules = fr.rules >| (tn{path = tn.path :> "foreignKey"}, foreignKey) >| (tn{path = tn.path :> "total"}, totality)
    }

lowerRealm :: Name -> C.Realm -> I.FlatRealm
lowerRealm realmName r = go I.emptyFlatRealm $ OMap.assocs generators
 where
  generators =
    OMap.fromList $
      for (toList r.generators) $ \(path, generator) ->
        (TableName realmName path, lowerGen generator)

  go fr [] = fr
  go fr ((tn, generator) : rest) =
    go (disaggregateGen generators tn generator fr) rest

writeIRFor :: C.Globals -> FilePath -> IO ()
writeIRFor ge fp = do
  forM_ (OMap.assocs ge.realms) $ \(x, r) -> do
    let fr = lowerRealm x r
    let fn = fp </> mangleToString x <> ".json"
    AE.encodeFile fn fr
    let pn = fp </> mangleToString x <> ".pretty"
    withFile pn WriteMode (\h -> hPutDoc h (dpretty fr))
