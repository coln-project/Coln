-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.SIR.Top where

import Control.Arrow ((&&&))
import Data.Map.Ordered qualified as OMap
import Data.Maybe (fromMaybe)

import Coln.Common
import Coln.Core.Params
import Coln.MIR.Realm qualified as MIR
import Coln.SIR.Cache
import Coln.SIR.Realm qualified as SIR
import Coln.SIR.Separate

split3 :: Dict (Maybe x, Maybe y, Maybe z) -> (Maybe (Dict x), Maybe (Dict y), Maybe (Dict z))
split3 d = do
  let d1 = case [(x, y) | (x, (Just y, _, _)) <- toList d] of
        [] -> Nothing
        pairs -> Just $ fromList pairs
  let d2 = case [(x, y) | (x, (_, Just y, _)) <- toList d] of
        [] -> Nothing
        pairs -> Just $ fromList pairs
  let d3 = case [(x, y) | (x, (_, _, Just y)) <- toList d] of
        [] -> Nothing
        pairs -> Just $ fromList pairs
  (d1, d2, d3)

aggregate3 ::
  (TableName -> a -> (Maybe (Trie x), Maybe (Trie y), Maybe (Trie z))) ->
  (TableName -> Trie a -> (Maybe (Trie x), Maybe (Trie y), Maybe (Trie z)))
aggregate3 f t (Leaf a) = f t a
aggregate3 f t (Node d) = do
  let (d1, d2, d3) = split3 $ aggregate3 f t <$> d
  (Node <$> d1, Node <$> d2, Node <$> d3)

cleanTrie :: Trie a -> Maybe (Trie a)
cleanTrie y@Leaf{} = Just y
cleanTrie (Node d) = case [(x, y) | (x, Just y) <- toList $ fmap cleanTrie d] of
  [] -> Nothing
  pairs -> Just $ Node $ fromList pairs

fromNode :: Maybe (Trie a) -> [(Name, Trie a)]
fromNode Nothing = []
fromNode (Just Leaf{}) = panic "leaf at top of generator trie"
fromNode (Just (Node d)) = toList d

mirToSIR :: RealmId -> MIR.Realm -> SIR.Realm
mirToSIR rId r = do
  let (_, _, root) = cache "root" (BwdNil :> "root") (emptyScope rId) r.root
  let (rootE, rootD, rootR) = aggregate3 separateGenerator (TableName rId $ BwdNil) r.generators
  let (names, cached) = unzip $ map (fst &&& uncurry (cacheTop rId)) $ OMap.assocs r.realmDefinitions
  let (cachedE, cachedD, _) = unzip3 cached
  let viewE = Node $ fromList [(x, y) | (x, Just y) <- zip names (map cleanTrie cachedE)]
  let viewD = Node $ fromList [(x, y) | (x, Just y) <- zip names (map cleanTrie cachedD)]
  SIR.Realm
    { entities = Node $ fromList $ fromNode rootE ++ [("view", viewE)]
    , definitions = Node $ fromList $ fromNode rootD ++ [("view", viewD)]
    , rules = fromMaybe emptyNode rootR
    , root = root
    , rootType = r.rootType
    }
