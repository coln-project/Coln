module Coln.FLIR.Flatten where

import Coln.Common
import Coln.Core.Params
import Coln.FLIR.Value qualified as V
import Coln.SIR.Syntax qualified as S

import Control.Monad (forM)
import Control.Monad.State

import Data.Set qualified as Set

newtype Els e = Els { unEls :: Trie (V.El e) }

leaf :: V.El e -> Els e
leaf = Els . Leaf

getLeaf :: Els e -> V.El e
getLeaf (Els (Leaf v)) = v
getLeaf _ = panic "tried to get leaf value of non-leaf"

node :: Dict a -> [Els e] -> Els e
node d vs = Els $ Node $ Dict d.head (fromList $ (.unEls) <$> vs)

proj :: Els e -> Name -> Els e
proj (Els (Node fields)) x = Els $ elemAt fields x
proj (Els (Leaf _)) _ = panic "tried to project from non-node"

concatEls :: [Els e] -> [V.El e]
concatEls vs = toList $ go vs BwdNil
  where
    go [] vs' = vs'
    go ((Els (Leaf v)):rest) vs' = go rest (vs' :> v)
    go ((Els (Node d)):rest) vs' = go rest (go (Els <$> toList d.values) vs')

newtype Props e = Props { apply :: Bwd (V.Prop e) -> Bwd (V.Prop e) }

instance Semigroup (Props e) where
  ps0 <> ps1 = Props (ps1.apply . ps0.apply)

instance Monoid (Props e) where
  mempty = Props id

single :: V.Prop e -> Props e
single p = Props (:> p)

data AuxilaryVars e = AuxilaryVars
  { vars :: Bwd (V.ColName, V.ColType)
  , props :: Bwd (V.Prop e)
  , length :: Int
  , usedRoots :: Set.Set Name
  }

newtype FlatM e a = FlatM {unFlatM :: State (AuxilaryVars e) a}
  deriving (Functor, Applicative, Monad, MonadState (AuxilaryVars e))


freshAt :: Path -> S.Shape -> FlatM e (Els e)
freshAt p = \case
  S.Scalar t -> do
    aux <- get
    let i = aux.length
    put $ aux { vars = (aux.vars :> (p, t)), length = (i + 1) }
    pure $ leaf $ V.LocalVar $ FId i
  S.Tuple fields -> do
    fields' <- forM (toList fields) $ \(x, sh) -> freshAt (p :> x) sh
    pure $ node fields fields'

fresh :: Maybe Name -> S.Shape -> FlatM e (Els e)
fresh mx sh = do
  x <- case mx of
    Just x -> pure x
    Nothing -> do
      aux <- get
      let x = freshNameFor aux.usedRoots
      put $ aux {usedRoots = Set.insert x aux.usedRoots}
      pure x
  freshAt (BwdNil :> x) sh
    
type Locals e = Bwd (Els e)

class Flatten a (b :: Type -> Type) | a -> b where
  flatten :: Locals e -> a -> FlatM e (b e)

absName :: S.Abs a -> Maybe Name
absName (S.Abs mx _) = mx
absName (S.AbsConst _) = Nothing

assert :: Props e -> FlatM e ()
assert ps = modify (\aux -> aux { props = ps.apply aux.props })

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
      pure $ node fields fields'
    S.Lit l -> pure $ leaf $ V.Lit l

equate :: S.Shape -> Els e -> Els e -> Props e
equate (S.Scalar _) v0 v1 = single $ V.PEq (getLeaf v0) (getLeaf v1)
equate (S.Tuple fs) v0 v1 =
  mconcat [ equate t (proj v0 x) (proj v1 x) | (x, t) <- toList fs ]

instance Flatten S.Prop Props where
  flatten l = \case
    S.Atom tn mt args -> do
      mv <- traverse (\t -> getLeaf <$> flatten l t) mt
      argvs <- traverse (flatten l) args
      pure $ single $ V.PAtom (V.Atom tn mv (Just <$> concatEls argvs))
    S.And ps -> mconcat <$> traverse (flatten l) (toList ps.values)
    S.Eq sh t0 t1 -> do
      v0 <- flatten l t0
      v1 <- flatten l t1
      pure $ equate sh v0 v1

