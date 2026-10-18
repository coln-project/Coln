-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Util.Containers where

import Data.List.NonEmpty (NonEmpty)
import Data.List.NonEmpty qualified as NE
import Data.Map.Ordered (OMap)
import Data.Map.Ordered qualified as OMap
import Data.Vector.Strict (Vector)
import Data.Vector.Strict qualified as V

import Coln.Util.Debug

class ElemAt a i b | a i -> b where
  elemAt :: (Dbg) => a -> i -> b

instance (Ord a) => ElemAt (OMap a b) a b where
  elemAt m k = case OMap.lookup k m of
    Just v -> v
    Nothing -> panic "no such key found in map"

class Lookup a i b | a -> i b where
  lookup :: a -> i -> Maybe b

instance (Ord a) => Lookup (OMap a b) a b where
  lookup m k = OMap.lookup k m

class Contains a i | a -> i where
  contains :: a -> i -> Bool

class ToList a e | a -> e where
  toList :: a -> [e]

instance ToList (V.Vector a) a where
  toList = V.toList

instance ToList (NonEmpty a) a where
  toList = NE.toList

class FromList a e | a -> e where
  fromList :: [e] -> a

instance FromList (Vector a) a where
  fromList = V.fromList
