module Coln.Util.Dict where

import Prelude hiding (lookup)

import FNotation (Name)
import Data.Vector.Strict qualified as V
import Data.Vector.Strict (Vector)
import Data.Vector.Fusion.Bundle qualified as Bundle
import Data.Vector.Generic (stream, unstreamM)
import Data.Map.Strict qualified as Map
import Data.Map.Strict (Map)
import Data.Key (Key, Keyed (..), FoldableWithKey (..), TraversableWithKey (..))

import Coln.Util.Debug
import Coln.Util.Containers

data DictHead = DictHead
  { byName :: Map Name Int
  , keys :: Vector Name
  }

data Dict a = Dict
  { head :: DictHead
  , values :: Vector a
  }

instance (Show a) => Show (Dict a) where
  show d = "Dict " ++ show (toList d)

dictLength :: Dict a -> Int
dictLength d = V.length d.values

instance Lookup (Dict a) Name a where
  lookup d x = (d.values V.!) <$> Map.lookup x d.head.byName

instance ElemAt (Dict a) Name a where
  elemAt t x = unwrap $ lookup t x

instance FromList (Dict a) (Name, a) where
  fromList pairs = do
    let keys = V.fromList $ fst <$> pairs
    let values = V.fromList $ snd <$> pairs
    let byName = Map.fromList $ zip (fst <$> pairs) [0 ..]
    Dict (DictHead byName keys) values

instance Functor Dict where
  fmap f d = Dict d.head (fmap f d.values)

instance Foldable Dict where
  foldMap f d = foldMap f d.values
  foldr f s d = foldr f s d.values
  foldl' f s d = foldl' f s d.values

instance Traversable Dict where
  traverse f d = fmap (\x -> d{values = x}) $ traverse f d.values

instance ToList (Dict a) (Name, a) where
  toList d = zip (V.toList d.head.keys) (V.toList d.values)

newtype KeyIndex = KeyIndex {value :: Int}

instance ElemAt (Dict a) KeyIndex a where
  elemAt d (KeyIndex i) = d.values V.! i

instance Contains (Dict a) Name where
  contains d x = Map.member x d.head.byName

getKeyIndex :: Dict a -> Name -> KeyIndex
getKeyIndex d x = KeyIndex $ d.head.byName Map.! x

withHead :: Dict a -> [b] -> Dict b
withHead d xs = do
  let n = V.length d.values
  Dict d.head (V.fromListN n xs)

dstream :: Dict a -> Bundle.Bundle V.Vector (Name, a)
dstream d = Bundle.zip (stream d.head.keys) (stream d.values)

type instance Key Dict = Name

instance Keyed Dict where
  mapWithKey f d = Dict d.head $ V.zipWith f d.head.keys d.values

instance FoldableWithKey Dict where
  toKeyedList = toList
  foldMapWithKey f = foldlWithKey (\acc x v -> acc <> f x v) mempty
  foldrWithKey f init =
    Bundle.foldr (uncurry f) init . dstream
  foldlWithKey f init =
    Bundle.foldl (\acc (x, v) -> f acc x v) init . dstream

instance TraversableWithKey Dict where
  traverseWithKey f d =
    withHead d
      <$> traverse (uncurry f) (zip (toList d.head.keys) (toList d.values))
  mapWithKeyM f d =
    Dict d.head
      <$> (unstreamM $ Bundle.mapM (uncurry f) $ dstream d)
