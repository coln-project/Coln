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

mirToSIR :: RealmId -> MIR.Realm -> SIR.Realm
mirToSIR rId r = do
  let root = separate 0 r.root
  let (rootE, rootD, rootR) = aggregate3 separateGenerator (TableName rId $ BwdNil :> "root") r.generators
  let (names, cached) = unzip $ map (fst &&& uncurry (cacheTop rId)) $ OMap.assocs r.realmDefinitions
  let (cachedE, cachedD, _) = unzip3 cached
  SIR.Realm
    { entities = Node $ fromList [(x, y) | (x, Just y) <- ("root", rootE) : zip names (map cleanTrie cachedE)]
    , definitions = Node $ fromList [(x, y) | (x, Just y) <- ("root", rootD) : zip names (map cleanTrie cachedD)]
    , rules = Node $ fromList [("root", fromMaybe emptyNode rootR)]
    , root = root
    , rootType = r.rootType
    }
