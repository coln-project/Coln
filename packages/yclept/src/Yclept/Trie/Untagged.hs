-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

{- | Untagged tries, a port of the OCaml @UntaggedTrie@ (@Trie.Untagged@).

An untagged trie is a @'Trie.Trie' d ()@ viewed through an API that hides the
unit tag.  Intended to be imported qualified, e.g.

> import qualified Yclept.Trie.Untagged as UntaggedTrie
-}
module Yclept.Trie.Untagged (
  Untagged,
  Path,
  BwdPath,

  -- * Construction
  empty,
  isEmpty,
  root,
  rootOpt,
  prefix,
  singleton,
  equal,

  -- * Finding
  findSubtree,
  findSingleton,
  findRoot,

  -- * Mapping and filtering
  iter,
  mapWithPath,
  filterWithPath,
  filterMapWithPath,

  -- * Updating
  updateSubtree,
  updateSingleton,
  updateRoot,

  -- * Union
  union,
  unionSubtree,
  unionSingleton,
  unionRoot,

  -- * Separation
  detachSubtree,
  detachSingleton,
  detachRoot,

  -- * Conversion
  toSeq,
  toSeqWithBwdPaths,
  toSeqValues,
  ofSeq,
  ofSeqWithMerger,

  -- * Tags
  tag,
  untag,
) where

import Data.Bifunctor (first, second)

import Yclept.Trie (BwdPath, Path, Trie, Untagged)
import Yclept.Trie qualified as Trie

tagV :: d -> (d, ())
tagV d = (d, ())

untagV :: (d, ()) -> d
untagV (d, ()) = d

-- The merger over untagged values, lifted to operate on @(d, ())@ pairs.
liftMerger :: (BwdPath -> d -> d -> d) -> BwdPath -> (d, ()) -> (d, ()) -> (d, ())
liftMerger m p x y = tagV (m p (untagV x) (untagV y))

empty :: Untagged d
empty = Trie.empty

isEmpty :: Untagged d -> Bool
isEmpty = Trie.isEmpty

root :: d -> Untagged d
root d = Trie.root (tagV d)

rootOpt :: Maybe d -> Untagged d
rootOpt md = Trie.rootOpt (tagV <$> md)

prefix :: Path -> Untagged d -> Untagged d
prefix = Trie.prefix

singleton :: (Path, d) -> Untagged d
singleton (p, d) = Trie.singleton (p, tagV d)

equal :: (Eq d) => Untagged d -> Untagged d -> Bool
equal = Trie.equal

findSubtree :: Path -> Untagged d -> Untagged d
findSubtree = Trie.findSubtree

findSingleton :: Path -> Untagged d -> Maybe d
findSingleton p t = untagV <$> Trie.findSingleton p t

findRoot :: Untagged d -> Maybe d
findRoot t = untagV <$> Trie.findRoot t

iter :: (Applicative m) => BwdPath -> (BwdPath -> d -> m ()) -> Untagged d -> m ()
iter pfx f = Trie.iter pfx (\p x -> f p (untagV x))

mapWithPath :: BwdPath -> (BwdPath -> d1 -> d2) -> Untagged d1 -> Untagged d2
mapWithPath pfx f = Trie.mapWithPath pfx (\p x -> tagV (f p (untagV x)))

filterWithPath :: BwdPath -> (BwdPath -> d -> Bool) -> Untagged d -> Untagged d
filterWithPath pfx f = Trie.filterWithPath pfx (\p x -> f p (untagV x))

filterMapWithPath :: BwdPath -> (BwdPath -> d1 -> Maybe d2) -> Untagged d1 -> Untagged d2
filterMapWithPath pfx f = Trie.filterMapWithPath pfx (\p x -> tagV <$> f p (untagV x))

updateSubtree :: Path -> (Untagged d -> Untagged d) -> Untagged d -> Untagged d
updateSubtree = Trie.updateSubtree

updateSingleton :: Path -> (Maybe d -> Maybe d) -> Untagged d -> Untagged d
updateSingleton p f = Trie.updateSingleton p (\md -> tagV <$> f (untagV <$> md))

updateRoot :: (Maybe d -> Maybe d) -> Untagged d -> Untagged d
updateRoot f = Trie.updateRoot (\md -> tagV <$> f (untagV <$> md))

union :: BwdPath -> (BwdPath -> d -> d -> d) -> Untagged d -> Untagged d -> Untagged d
union pfx m = Trie.union pfx (liftMerger m)

unionSubtree :: BwdPath -> (BwdPath -> d -> d -> d) -> Untagged d -> (Path, Untagged d) -> Untagged d
unionSubtree pfx m = Trie.unionSubtree pfx (liftMerger m)

unionSingleton :: BwdPath -> (BwdPath -> d -> d -> d) -> Untagged d -> (Path, d) -> Untagged d
unionSingleton pfx m t (p, d) = Trie.unionSingleton pfx (liftMerger m) t (p, tagV d)

unionRoot :: BwdPath -> (BwdPath -> d -> d -> d) -> Untagged d -> d -> Untagged d
unionRoot pfx m t d = Trie.unionRoot pfx (liftMerger m) t (tagV d)

detachSubtree :: Path -> Untagged d -> (Untagged d, Untagged d)
detachSubtree = Trie.detachSubtree

detachSingleton :: Path -> Untagged d -> (Maybe d, Untagged d)
detachSingleton p t = first (fmap untagV) (Trie.detachSingleton p t)

detachRoot :: Untagged d -> (Maybe d, Untagged d)
detachRoot t = first (fmap untagV) (Trie.detachRoot t)

toSeq :: BwdPath -> Untagged d -> [(Path, d)]
toSeq pfx = map (second untagV) . Trie.toSeq pfx

toSeqWithBwdPaths :: BwdPath -> Untagged d -> [(BwdPath, d)]
toSeqWithBwdPaths pfx = map (second untagV) . Trie.toSeqWithBwdPaths pfx

toSeqValues :: Untagged d -> [d]
toSeqValues = map untagV . Trie.toSeqValues

ofSeq :: [(Path, d)] -> Untagged d
ofSeq = Trie.ofSeq . map (second tagV)

ofSeqWithMerger :: BwdPath -> (BwdPath -> d -> d -> d) -> [(Path, d)] -> Untagged d
ofSeqWithMerger pfx m = Trie.ofSeqWithMerger pfx (liftMerger m) . map (second tagV)

-- | Attach a tag to every binding of an untagged trie (OCaml @tag@).
tag :: t -> Untagged d -> Trie d t
tag = Trie.retag

-- | Forget all tags (OCaml @untag@).
untag :: Trie d t -> Untagged d
untag = Trie.untag
