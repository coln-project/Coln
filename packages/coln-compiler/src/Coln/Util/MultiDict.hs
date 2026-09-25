-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Util.MultiDict where

import Prelude hiding (lookup)

import Data.Key (FoldableWithKey (..), Key, Keyed (..), TraversableWithKey (..))
import Data.Map.Strict (Map)
import Data.Map.Strict qualified as Map
import Data.Vector.Fusion.Bundle qualified as Bundle
import Data.Vector.Generic (stream, unstreamM)
import Data.Vector.Strict (Vector)
import Data.Vector.Strict qualified as V
import FNotation (Name)

import Coln.Util.Containers
import Coln.Util.Debug
import Coln.Util.Dict

-- MultiDict with grouped Names
--------------------------------------------------------------------------------

data MultiDictHead = MultiDictHead
  { byName :: Map Name Int
  , keys :: Vector [Name]
  }

data MultiDict a = MultiDict
  { head :: MultiDictHead
  , values :: Vector a
  }

expandMultiDict :: MultiDict a -> Dict a
expandMultiDict md = fromList [(name, value) | (names, value) <- toList md, name <- names]

instance (Show a) => Show (MultiDict a) where
  show d = "MultiDict " ++ show (toList d)

multiDictLength :: MultiDict a -> Int
multiDictLength d = V.sum $ fmap length d.head.keys

instance Lookup (MultiDict a) Name a where
  lookup d x = (d.values V.!) <$> Map.lookup x d.head.byName

instance ElemAt (MultiDict a) Name a where
  elemAt t x = unwrap $ lookup t x

instance FromList (MultiDict a) ([Name], a) where
  fromList pairs = do
    let keys = V.fromList $ fst <$> pairs
    let values = V.fromList $ snd <$> pairs
    let byName = Map.fromList $ concat $ zipWith (\names i -> zip names (repeat i)) (fst <$> pairs) [0 ..]
    MultiDict (MultiDictHead byName keys) values

instance Functor MultiDict where
  fmap f d = MultiDict d.head (fmap f d.values)

instance Foldable MultiDict where
  foldMap f d = foldMap f d.values
  foldr f s d = foldr f s d.values
  foldl' f s d = foldl' f s d.values
  toList d = V.toList d.values

instance Traversable MultiDict where
  traverse f d = fmap (\x -> d{values = x}) $ traverse f d.values

instance ToList (MultiDict a) ([Name], a) where
  toList d = zip (V.toList d.head.keys) (V.toList d.values)

instance ElemAt (MultiDict a) KeyIndex a where
  elemAt d (KeyIndex i) = d.values V.! i

instance Contains (MultiDict a) Name where
  contains d x = Map.member x d.head.byName

getKeyIndexMulti :: MultiDict a -> Name -> KeyIndex
getKeyIndexMulti d x = KeyIndex $ d.head.byName Map.! x
