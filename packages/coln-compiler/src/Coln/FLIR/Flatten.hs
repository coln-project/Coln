module Coln.FLIR.Flatten where

import Coln.Common
import Coln.Core.Params
import Coln.FLIR.Value qualified as V
import Coln.SIR.Realm qualified as S
import Coln.SIR.Syntax qualified as S

import Control.Monad (forM)
import Control.Monad.State
import Data.Set qualified as Set
import Data.Vector.Strict qualified as Vec

data Els e
  = Scalar (V.El e)
  | Cons (Dict (Els e))
  | Erased

getScalar :: Els e -> V.El e
getScalar (Scalar v) = v
getScalar _ = panic "tried to get leaf value of non-leaf"

asAtomHead :: Els e -> Maybe (V.El e)
asAtomHead (Scalar v) = Just v
asAtomHead Erased = Nothing
asAtomHead _ = panic "tried to get leaf value of non-leaf"

proj :: Els e -> Name -> Els e
proj (Cons fields) x = elemAt fields x
proj Erased _ = Erased
proj (Scalar _) _ = panic "tried to project from non-node"

concatEls :: [Els e] -> [V.El e]
concatEls vs = toList $ go vs BwdNil
 where
  go [] vs' = vs'
  go (Scalar v : rest) vs' = go rest (vs' :> v)
  go (Cons d : rest) vs' = go rest (go (toList d.values) vs')
  go (Erased : rest) vs' = go rest vs'

newtype Props e = Props {apply :: Bwd (V.Prop e) -> Bwd (V.Prop e)}

instance Semigroup (Props e) where
  ps0 <> ps1 = Props (ps1.apply . ps0.apply)

instance Monoid (Props e) where
  mempty = Props id

instance ToList (Props e) (V.Prop e) where
  toList ps = toList (ps.apply BwdNil)

single :: V.Prop e -> Props e
single p = Props (:> p)

data AuxilaryVars e = AuxilaryVars
  { vars :: Bwd (V.ColName, V.ColType)
  , props :: Props e
  , length :: Int
  , usedRoots :: Set.Set Name
  }

newtype FlatM e a = FlatM {unFlatM :: State (AuxilaryVars e) a}
  deriving (Functor, Applicative, Monad, MonadState (AuxilaryVars e))

runFlatM :: FlatM e a -> (a, [(V.ColName, V.ColType)], Props e)
runFlatM action = do
  let (x, aux) = runState action.unFlatM (AuxilaryVars BwdNil mempty 0 Set.empty)
  (x, toList aux.vars, aux.props)

freshAt :: Path -> S.Shape -> FlatM e (Els e)
freshAt p = \case
  S.Scalar t -> do
    aux <- get
    let i = aux.length
    put $ aux{vars = (aux.vars :> (p, t)), length = (i + 1)}
    pure $ Scalar $ V.LocalVar $ FId i
  S.Tuple fields -> do
    fields' <- forM (toList fields) $ \(x, sh) -> freshAt (p :> x) sh
    pure $ Cons $ withHead fields fields'
  S.Unstored -> pure Erased

fresh :: Maybe Name -> S.Shape -> FlatM e (Els e)
fresh mx sh = do
  x <- case mx of
    Just x -> pure x
    Nothing -> do
      aux <- get
      let x = freshNameFor aux.usedRoots
      put $ aux{usedRoots = Set.insert x aux.usedRoots}
      pure x
  freshAt (BwdNil :> x) sh

type Locals e = Bwd (Els e)

class Flatten a (b :: Type -> Type) | a -> b where
  flatten :: Locals e -> a -> FlatM e (b e)

absName :: S.Abs a -> Maybe Name
absName (S.Abs mx _) = mx
absName (S.AbsConst _) = Nothing

assert :: Props e -> FlatM e ()
assert ps = modify (\aux -> aux{props = aux.props <> ps})

app :: (Flatten a b) => Locals e -> S.Abs a -> Els e -> FlatM e (b e)
app l (S.Abs _ body) v = flatten (l :> v) body
app l (S.AbsConst body) _ = flatten l body

instance Flatten (S.El Set) Els where
  flatten l = \case
    S.Var i -> pure $ elemAt l i
    S.Single q -> do
      v <- fresh (absName q.pred) q.shape
      app l q.pred v >>= assert
      pure v
    S.Proj t x -> do
      v <- flatten l t
      pure $ proj v x
    S.Cons fields -> do
      fields' <- forM (toList fields) $ \(_, t) -> flatten l t
      pure $ Cons $ withHead fields fields'
    S.Lit l -> pure $ Scalar $ V.Lit l
    S.Erased -> pure Erased

equate :: S.Shape -> Els e -> Els e -> Props e
equate (S.Scalar _) v0 v1 = single $ V.PEq (getScalar v0) (getScalar v1)
equate (S.Tuple fs) v0 v1 =
  mconcat [equate t (proj v0 x) (proj v1 x) | (x, t) <- toList fs]
equate S.Unstored _ _ = mempty

instance Flatten S.Prop Props where
  flatten l = \case
    S.Atom tn t args -> do
      mv <- asAtomHead <$> flatten l t
      argvs <- traverse (flatten l) args
      pure $ single $ V.PAtom (V.Atom tn mv (Just <$> concatEls argvs))
    S.And ps -> mconcat <$> traverse (flatten l) (toList ps.values)
    S.Eq sh t0 t1 -> do
      v0 <- flatten l t0
      v1 <- flatten l t1
      pure $ equate sh v0 v1

flattenColumn :: V.ColName -> S.Shape -> [(V.ColName, V.ColType)]
flattenColumn p = \case
  S.Scalar t -> [(p, t)]
  S.Tuple d -> concat [flattenColumn (p :> x) t | (x, t) <- toList d]
  S.Unstored -> []

flattenColumns :: [(Name, S.Shape)] -> [(V.ColName, V.ColType)]
flattenColumns = concat . fmap (\(x, sh) -> flattenColumn (BwdNil :> x) sh)

flattenPrimaryKey :: [S.Shape] -> [Int] -> [Int]
flattenPrimaryKey shapes cols = do
  let go n [] = [n]
      go n (sh : rest) = do
        let s = S.shapeSize sh
        n : go (n + s) rest
  let offsets = Vec.fromList $ go 0 shapes
  concat [[(offsets Vec.! i) .. (offsets Vec.! (i + 1)) - 1] | i <- cols]

flattenEntity :: S.Entity -> V.Entity
flattenEntity e =
  V.Entity
    { V.entityVariant = case e.entityVariant of
        S.Table -> V.Table
        S.View -> V.View V.Materialized
    , V.columns = flattenColumns e.columns
    , V.primaryKey = fmap (flattenPrimaryKey (snd <$> e.columns)) e.primaryKey
    }

bindTele :: Locals e -> [(Name, S.Query)] -> FlatM e (Locals e)
bindTele l [] = pure l
bindTele l ((x, a) : rest) = do
  v <- fresh (Just x) a.shape
  app l a.pred v >>= assert
  bindTele (l :> v) rest

flattenRule :: S.Rule -> V.Rule
flattenRule r = do
  let ((ante, cons), vars, ps) = runFlatM $ do
        vs <- bindTele BwdNil r.inCtx
        ante <- flatten vs r.antecedent
        cons <- flatten vs r.consequent
        pure (ante, cons)
  V.Rule
    { V.ruleVariant = r.ruleVariant
    , V.vars = vars
    , V.antecedents = toList (ps <> ante)
    , V.consequents = toList cons
    }

flattenDefinition :: S.Definition -> V.Definition
flattenDefinition d = do
  let (args, vars, ps) = runFlatM $ do
        vs <- bindTele BwdNil d.inCtx
        concatEls <$> traverse (flatten vs) d.args
  V.Definition
    { V.vars = vars
    , V.antecedents = toList ps
    , V.definand = d.definand
    , V.args = args
    }
