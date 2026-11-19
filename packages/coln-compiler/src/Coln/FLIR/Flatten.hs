-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.FLIR.Flatten where

import Coln.Common
import Coln.Core.Params
import Coln.FLIR.Value qualified as V
import Coln.SIR.Realm qualified as S
import Coln.SIR.Syntax qualified as S

import Control.Monad (forM)
import Control.Monad.State
import Data.Set qualified as Set


data Els
  = Scalar V.El
  | Bundle (Dict Els)
  | Erased

asVarName :: Path -> V.LocalName
asVarName p = V.LocalName (manglePath p).name

proj :: Els -> Name -> Els
proj (Bundle fields) x = elemAt fields x
proj Erased _ = Erased
proj (Scalar _) _ = panic "tried to project from non-extern scalar"

concatEls :: [Els] -> [V.El]
concatEls vs = toList $ go vs BwdNil
 where
  go [] vs' = vs'
  go (Scalar v : rest) vs' = go rest (vs' :> v)
  go (Bundle d : rest) vs' = go rest (go (toList d.values) vs')
  go (Erased : rest) vs' = go rest vs'

newtype Props = Props {apply :: Bwd V.Prop -> Bwd V.Prop}

instance Semigroup Props where
  ps0 <> ps1 = Props (ps1.apply . ps0.apply)

instance Monoid Props where
  mempty = Props id

instance ToList Props V.Prop where
  toList ps = toList (ps.apply BwdNil)

single :: V.Prop -> Props
single p = Props (:> p)

data AuxilaryVars = AuxilaryVars
  { vars :: Bwd (V.LocalName, V.ValType)
  , numVars :: Int
  , props :: Props
  , usedRoots :: Set.Set Name
  }

newtype FlatM a = FlatM {unFlatM :: State AuxilaryVars a}
  deriving (Functor, Applicative, Monad, MonadState AuxilaryVars)

runFlatM :: FlatM a -> (a, [(V.LocalName, V.ValType)], Props)
runFlatM action = do
  let (x, aux) = runState action.unFlatM (AuxilaryVars BwdNil 0 mempty Set.empty)
  (x, toList aux.vars, aux.props)

freshAt :: Path -> S.Shape -> FlatM Els
freshAt p = \case
  S.Scalar t -> do
    aux <- get
    let i = aux.numVars
    put $ aux{vars = (aux.vars :> (asVarName p, t)), numVars = (i + 1)}
    pure $ Scalar $ V.LocalVar $ FId i
  S.Tuple fields -> do
    fields' <- forM (toList fields) $ \(x, sh) -> freshAt (p :> x) sh
    pure $ Bundle $ withHead fields fields'
  S.Unstored -> pure Erased

fresh :: Maybe Name -> S.Shape -> FlatM Els
fresh mx sh = do
  aux <- get
  let x = case mx of
        Just x -> freshenFor aux.usedRoots x
        Nothing -> freshNameFor aux.usedRoots
  put $ aux{usedRoots = Set.insert x aux.usedRoots}
  freshAt (BwdNil :> x) sh

getScalar :: Els -> V.El
getScalar (Scalar v) = v
getScalar _ = panic "tried to get leaf value of non-leaf"

asAtomHead :: Els -> Maybe V.El
asAtomHead (Scalar v) = Just v
asAtomHead Erased = Nothing
asAtomHead _ = panic "tried to get leaf value of non-leaf"

type Locals = Bwd Els

absName :: S.Abs a -> Maybe Name
absName (S.Abs mx _) = mx
absName (S.AbsConst _) = Nothing

assert :: Props -> FlatM ()
assert ps = modify (\aux -> aux{props = aux.props <> ps})

class Flatten a b | a -> b where
  flatten :: Locals -> a -> FlatM b

app :: (Flatten a b) => Locals -> S.Abs a -> Els -> FlatM b
app l (S.Abs _ body) v = flatten (l :> v) body
app l (S.AbsConst body) _ = flatten l body

instance Flatten (S.El Set) Els where
  flatten l = \case
    S.Var i -> pure $ elemAt l i
    S.Lookup tn args shape -> do
      v <- fresh Nothing shape
      args' <- traverse (flatten l) args
      assert $ single $ V.PAtom $ V.Atom tn Nothing (concatEls (args' ++ [v]))
      pure v
    S.Proj t x -> do
      v <- flatten l t
      pure $ proj v x
    S.Bundle fields -> do
      fields' <- forM (toList fields) $ \(_, t) -> flatten l t
      pure $ Bundle $ withHead fields fields'
    S.Lit l -> pure $ Scalar $ V.Lit l
    S.Erased -> pure Erased

equate :: S.Shape -> Els -> Els -> Props
equate (S.Scalar ty) v0 v1 = single $ V.PEq ty (getScalar v0) (getScalar v1)
equate (S.Tuple fs) v0 v1 =
  mconcat [equate t (proj v0 x) (proj v1 x) | (x, t) <- toList fs]
equate S.Unstored _ _ = mempty

instance Flatten S.Prop Props where
  flatten l = \case
    S.Atom tn t args -> do
      mv <- asAtomHead <$> flatten l t
      argvs <- traverse (flatten l) args
      pure $ single $ V.PAtom (V.Atom tn mv (concatEls argvs))
    S.And ps -> mconcat <$> traverse (flatten l) (toList ps.values)
    S.Eq sh t0 t1 -> do
      v0 <- flatten l t0
      v1 <- flatten l t1
      pure $ equate sh v0 v1

instance Flatten [S.Prop] Props where
  flatten l ps = mconcat <$> mapM (flatten l) ps

flattenColumn :: Path -> S.Shape -> [(V.LocalName, V.ValType)]
flattenColumn p = \case
  S.Scalar t -> [(asVarName p, t)]
  S.Tuple d -> concat [flattenColumn (p :> x) t | (x, t) <- toList d]
  S.Unstored -> []

flattenColumns :: [(Name, S.Shape)] -> [(V.LocalName, V.ValType)]
flattenColumns = concat . fmap (\(x, sh) -> flattenColumn (BwdNil :> x) sh)

flattenPrimaryKey :: [S.Shape] -> S.PrimaryKey -> V.PrimaryKey
flattenPrimaryKey shapes (S.PrimaryKey n) =
  V.PrimaryKey $ sum [S.shapeSize sh | sh <- take n shapes]

flattenTable :: S.Table -> V.Table
flattenTable t =
  V.Table
    { V.tableVariant = t.tableVariant
    , V.columns = V.Context $ flattenColumns t.columns
    , V.primaryKey = fmap (flattenPrimaryKey (snd <$> t.columns)) t.primaryKey
    }

bindTele :: Locals -> [(Name, S.Shape)] -> FlatM Locals
bindTele l [] = pure l
bindTele l ((x, a) : rest) = do
  v <- fresh (Just x) a
  bindTele (l :> v) rest

flattenLaw :: S.Law -> V.Law
flattenLaw r = do
  let ((ante, cons), vars, ps) = runFlatM $ do
        vs <- bindTele BwdNil r.inCtx
        ante <- flatten vs r.antecedent
        cons <- flatten vs r.consequent
        pure (ante, cons)
  let (ruleAnte, ruleCons) = (ps <> ante, cons)
  V.Law
    { V.lawVariant = r.ruleVariant
    , V.context = V.Context vars
    , V.antecedents = toList ruleAnte
    , V.consequents = toList ruleCons
    }

flattenRule :: S.Rule -> V.Rule
flattenRule r = do
  let ((ante, args), vars, ps) = runFlatM $ do
        vs <- bindTele BwdNil r.inCtx
        ante <- flatten vs r.antecedent
        args <- concatEls <$> traverse (flatten vs) r.args
        pure (ante, args)
  V.Rule
    { V.context = V.Context vars
    , V.antecedents = toList (ps <> ante)
    , V.definand = r.definand
    , V.args = args
    }
