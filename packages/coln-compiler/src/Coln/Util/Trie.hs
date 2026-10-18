-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Util.Trie where

import FNotation

import Coln.Util.Bwd
import Coln.Util.Containers
import Coln.Util.Dict

data Trie a
  = Leaf a
  | Node (Dict (Trie a))
  deriving (Functor, Foldable, Traversable)

instance ToList (Trie a) (Bwd Name, a) where
  toList = go BwdNil
   where
    go prefix = \case
      Leaf x -> [(prefix, x)]
      Node ts -> concat $ [go (prefix :> x) t | (x, t) <- toList ts]
