-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT
{-# LANGUAGE OverloadedRecordDot #-}
{-# LANGUAGE ScopedTypeVariables #-}

{- | A persistent trie keyed by hierarchical names (@['Data.Text.Text']@ paths),
a port of the OCaml @Trie@ module.

Each binding carries a @data@ payload and a @tag@.  The data survives
retagging; tags can be reset in @O(1)@ (see 'retag').  Internally the trie
keeps a data tree and a sparse tag tree that mirrors it, with a
@tagDefaultChild@ standing in for \"every child not listed explicitly has
this tag\" so that 'retag' is constant time.

The public merge/update API comes in a pure flavour and a monadic flavour
(the @*M@ functions).  The monadic variants exist because the modifier
engine's @shadow@ merger runs in a base monad @m@; see "Yclept.Modifier".
-}
module Yclept.Trie (
  -- * Types
  Path,
  BwdPath,
  Trie,
  Untagged,

  -- * Basic construction
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
  updateSubtreeM,
  updateSingleton,
  updateRoot,

  -- * Union (pure)
  union,
  unionSubtree,
  unionSingleton,
  unionRoot,

  -- * Union (monadic merger)
  Merger,
  unionM,
  unionSubtreeM,
  unionSingletonM,
  unionRootM,

  -- * Separation
  detachSubtree,
  detachSingleton,
  detachRoot,

  -- * Conversion to\/from lists
  toSeq,
  toSeqWithBwdPaths,
  toSeqValues,
  ofSeq,
  ofSeqWithMerger,
  ofSeqWithMergerM,

  -- * Tags
  mapTag,
  retag,
  retagSubtree,
  untag,
  setOfTags,
) where

import Control.Monad (foldM)
import Control.Monad.Trans.Writer.CPS (Writer, runWriter, writer)
import Data.Foldable (traverse_)
import Data.Functor.Identity (Identity (..))
import Data.Map.Merge.Strict qualified as Merge
import Data.Map.Strict (Map)
import Data.Map.Strict qualified as Map
import Data.Maybe (fromMaybe, isNothing, maybeToList)
import Data.Monoid (First (..))
import Data.Set (Set)
import Data.Set qualified as Set
import Data.Text (Text)

import Yclept.Bwd (Bwd (Emp), (<:), (<@))

-- | The type of hierarchical names.  The name @x.y.z@ is @[\"x\", \"y\", \"z\"]@.
type Path = [Text]

-- | Hierarchical names as backward lists.
type BwdPath = Bwd Text

type Seg = Text

type SegMap a = Map Seg a

-- The data tree: a payload at the root plus a map of named children.  This
-- representation is canonical, so a derived 'Eq' is the right structural
-- equality.
data DataNode a = DataNode
  { dRoot :: !(Maybe a)
  , dChildren :: !(SegMap (DataNode a))
  }
  deriving (Eq)

-- The tag tree mirrors the data tree.  Invariants (mirroring the OCaml):
--   1. @tagChildren@ is a subset of the corresponding @dChildren@.
--   2. @tagRoot@ is present exactly when @dRoot@ is.
--   3. @tagDefaultChild@ stands for the tag of every data child not present
--      in @tagChildren@.
-- The tag tree is /not/ canonical (the same logical tagging has several
-- representations), and its equality is only meaningful relative to the data
-- tree it decorates (a child's tag may be given explicitly on one side and via
-- @tagDefaultChild@ on the other).  So 'TagNode' has no 'Eq' of its own; the
-- data-aware comparison lives in the 'Eq' instance of 'Node' below.
data TagNode a = TagNode
  { tagRoot :: !(Maybe a)
  , tagDefaultChild :: !(Maybe a)
  , tagChildren :: !(SegMap (TagNode a))
  }

data Node d t = Node !(DataNode d) !(TagNode t)

-- Hand-written (non-derived) equality: the data tree compares structurally,
-- and the tag tree is compared /relative to that data tree/.
instance (Eq d, Eq t) => Eq (Node d t) where
  Node d1 t1 == Node d2 t2 = d1 == d2 && equalTagNode d1 t1 t2

{- | The abstract type of a trie.  @d@ is the data (survives retagging), @t@ is
the tag.
-}
type Trie d t = Maybe (Node d t)

-- | Untagged tries (all tags are @()@).
type Untagged d = Trie d ()

-- ---------------------------------------------------------------------------
-- Making (non-empty) trees
-- ---------------------------------------------------------------------------

empty :: Trie d t
empty = Nothing

isEmpty :: Trie d t -> Bool
isEmpty = isNothing

nonEmpty :: Node d t -> Trie d t
nonEmpty = Just

-- Normalise a tag node against its data node (invariants 2 and 3).
mkTagNode :: DataNode d -> (Maybe t, (Maybe t, SegMap (TagNode t))) -> TagNode t
mkTagNode d (tagRoot0, (tagDefaultChild0, tagChildren0)) =
  TagNode
    { tagRoot = case d.dRoot of
        Nothing -> Nothing
        Just _ -> tagRoot0
    , tagDefaultChild =
        if Map.size (d.dChildren) == Map.size tagChildren0
          then Nothing
          else tagDefaultChild0
    , tagChildren = tagChildren0
    }

mkTagNode' :: DataNode d -> Maybe t -> TagNode t
mkTagNode' d t = mkTagNode d (t, (t, Map.empty))

mkNode' :: DataNode d -> Maybe t -> Node d t
mkNode' d t = Node d (mkTagNode' d t)

-- Materialise @tagDefaultChild@ into explicit @tagChildren@ (invariant-safe).
dropTagDefaultChild :: Node d t -> Node d t
dropTagDefaultChild n@(Node d t) =
  case t.tagDefaultChild of
    Nothing -> n
    def@(Just _) ->
      let tagChildren' =
            Merge.merge
              (Merge.mapMissing (\_ dchild -> mkTagNode' dchild def))
              (Merge.mapMissing (\_ _ -> invariant)) -- tag child w/o data child
              (Merge.zipWithMatched (\_ _dchild tchild -> tchild))
              (d.dChildren)
              (t.tagChildren)
       in Node d (t{tagDefaultChild = Nothing, tagChildren = tagChildren'})

invariant :: a
invariant = error "Yclept.Trie: broken invariant (tag child without data child)"

mkTree :: (Maybe d, SegMap (DataNode d)) -> (Maybe t, (Maybe t, SegMap (TagNode t))) -> Trie d t
mkTree (r, children) tagParams
  | isNothing r && Map.null children = empty
  | otherwise =
      let d = DataNode{dRoot = r, dChildren = children}
       in nonEmpty (Node d (mkTagNode d tagParams))

rootNode :: (d, t) -> Node d t
rootNode (d, t) =
  Node
    (DataNode{dRoot = Just d, dChildren = Map.empty})
    (TagNode{tagRoot = Just t, tagDefaultChild = Nothing, tagChildren = Map.empty})

-- | @'root' (d, t)@ makes a trie with a single binding at the root.
root :: (d, t) -> Trie d t
root = nonEmpty . rootNode

-- | @'rootOpt' 'Nothing'@ is 'empty'; @'rootOpt' ('Just' v)@ is @'root' v@.
rootOpt :: Maybe (d, t) -> Trie d t
rootOpt = fmap rootNode

prefixNode :: Path -> Node d t -> Node d t
prefixNode path n = foldr f n path
 where
  f seg (Node d t) =
    Node
      (DataNode{dRoot = Nothing, dChildren = Map.singleton seg d})
      (TagNode{tagRoot = Nothing, tagDefaultChild = Nothing, tagChildren = Map.singleton seg t})

-- | @'prefix' p t@ makes a minimal trie with @t@ rooted at @p@.
prefix :: Path -> Trie d t -> Trie d t
prefix path = fmap (prefixNode path)

-- | @'singleton' (p, (d, t))@ makes a trie with the single binding @p@.
singleton :: (Path, (d, t)) -> Trie d t
singleton (path, dt) = prefix path (root dt)

-- ---------------------------------------------------------------------------
-- Small helpers
-- ---------------------------------------------------------------------------

-- Split a binding value (a data\/tag pair).
splitMaybe :: Maybe (a, b) -> (Maybe a, Maybe b)
splitMaybe Nothing = (Nothing, Nothing)
splitMaybe (Just (a, b)) = (Just a, Just b)

-- Split a whole subtree into its data node and tag node.  (In OCaml both this
-- and 'splitMaybe' were the same function, since a node was itself a tuple.)
splitNode :: Trie d t -> (Maybe (DataNode d), Maybe (TagNode t))
splitNode Nothing = (Nothing, Nothing)
splitNode (Just (Node d t)) = (Just d, Just t)

-- Materialise the children of a node as data\/tag pairs (invariant-safe).
getChildrenNode :: Node d t -> SegMap (Node d t)
getChildrenNode (Node d t) =
  Merge.merge
    (Merge.mapMissing (\_ dchild -> Node dchild (mkTagNode' dchild (t.tagDefaultChild))))
    (Merge.mapMissing (\_ _ -> invariant))
    (Merge.zipWithMatched (\_ dchild tchild -> Node dchild tchild))
    (d.dChildren)
    (t.tagChildren)

-- ---------------------------------------------------------------------------
-- Equality
-- ---------------------------------------------------------------------------

-- The data tree uses its derived 'Eq'.  Only the (non-canonical) tag tree
-- needs a hand-written traversal that materialises default children.

equalTagNode :: (Eq t) => DataNode d -> TagNode t -> TagNode t -> Bool
equalTagNode d t1 t2 =
  t1.tagRoot == t2.tagRoot && equalTagChildren d t1 t2

equalTagChildren :: (Eq t) => DataNode d -> TagNode t -> TagNode t -> Bool
equalTagChildren d t1 t2 =
  ( t1.tagDefaultChild == t2.tagDefaultChild
      && Map.null (t1.tagChildren)
      && Map.null (t2.tagChildren)
  )
    || all (\(dc, tc1, tc2) -> equalTagNode dc tc1 tc2) (Map.elems (children2 d t1 t2))

-- Line up the two tag trees against the shared data tree, materialising each
-- side's default children.
children2 :: DataNode d -> TagNode t -> TagNode t -> SegMap (DataNode d, TagNode t, TagNode t)
children2 d t1 t2 =
  Merge.merge
    (Merge.mapMissing (\_ (Node dc tc1) -> (dc, tc1, mkTagNode' dc (t2.tagDefaultChild))))
    (Merge.mapMissing (\_ _ -> invariant))
    (Merge.zipWithMatched (\_ (Node dc tc1) tc2 -> (dc, tc1, tc2)))
    (getChildrenNode (Node d t1))
    (t2.tagChildren)

{- | Structural equality on tries.  This is just the 'Eq' instance of the
underlying @'Maybe' ('Node' d t)@; provided under the OCaml name.
-}
equal :: (Eq d, Eq t) => Trie d t -> Trie d t -> Bool
equal = (==)

-- ---------------------------------------------------------------------------
-- Getting data
-- ---------------------------------------------------------------------------

findChildNode :: Seg -> Node d t -> Maybe (Node d t)
findChildNode seg (Node d t) =
  case Map.lookup seg (d.dChildren) of
    Nothing -> Nothing
    Just dc ->
      case Map.lookup seg (t.tagChildren) of
        Just tc -> Just (Node dc tc)
        Nothing -> Just (mkNode' dc (t.tagDefaultChild))

findNodeCont :: Path -> Node d t -> (Node d t -> Maybe b) -> Maybe b
findNodeCont [] n k = k n
findNodeCont (seg : path) n k =
  findChildNode seg n >>= \n' -> findNodeCont path n' k

findRootNode :: Node d t -> Maybe (d, t)
findRootNode (Node d t) =
  case d.dRoot of
    Nothing -> Nothing
    Just r -> Just (r, fromMaybe invariant (t.tagRoot))

-- | @'findSubtree' p t@ returns the subtree rooted at @p@.
findSubtree :: Path -> Trie d t -> Trie d t
findSubtree path v = v >>= \n -> findNodeCont path n nonEmpty

-- | @'findSingleton' p t@ returns the data and tag at @p@.
findSingleton :: Path -> Trie d t -> Maybe (d, t)
findSingleton path v = v >>= \n -> findNodeCont path n findRootNode

-- | @'findRoot' t@ returns the data and tag at the root.
findRoot :: Trie d t -> Maybe (d, t)
findRoot v = v >>= findRootNode

-- ---------------------------------------------------------------------------
-- Updating
-- ---------------------------------------------------------------------------

updateNodeContM :: (Monad m) => Path -> Node d t -> (Trie d t -> m (Trie d t)) -> m (Trie d t)
updateNodeContM [] n k = k (nonEmpty n)
updateNodeContM (seg : path) (Node d t) k = do
  childTrie <-
    case findChildNode seg (Node d t) of
      Nothing -> prefix path <$> k empty
      Just n -> updateNodeContM path n k
  let (child, tagChild) = splitNode childTrie
      children' = Map.alter (const child) seg (d.dChildren)
      tagChildren' = Map.alter (const tagChild) seg (t.tagChildren)
  pure (mkTree (d.dRoot, children') (t.tagRoot, (t.tagDefaultChild, tagChildren')))

updateContM :: (Monad m) => Path -> Trie d t -> (Trie d t -> m (Trie d t)) -> m (Trie d t)
updateContM path v k =
  case v of
    Nothing -> prefix path <$> k empty
    Just n -> updateNodeContM path n k

updateCont :: Path -> Trie d t -> (Trie d t -> Trie d t) -> Trie d t
updateCont path v k = runIdentity (updateContM path v (Identity . k))

-- | @'updateSubtree' p f t@ replaces the subtree rooted at @p@ with @f@ of it.
updateSubtree :: Path -> (Trie d t -> Trie d t) -> Trie d t -> Trie d t
updateSubtree path f v = updateCont path v f

{- | Monadic 'updateSubtree'; the replacement runs in @m@ (used by the modifier
engine, whose @in_@ recurses into a subtree effectfully).
-}
updateSubtreeM :: (Monad m) => Path -> (Trie d t -> m (Trie d t)) -> Trie d t -> m (Trie d t)
updateSubtreeM path f v = updateContM path v f

-- | @'updateRoot' f t@ updates the value at the root with @f@.
updateRoot :: (Maybe (d, t) -> Maybe (d, t)) -> Trie d t -> Trie d t
updateRoot f Nothing = rootOpt (f Nothing)
updateRoot f (Just (Node d t)) =
  let (r, tr) = splitMaybe (f (findRootNode (Node d t)))
   in mkTree (r, d.dChildren) (tr, (t.tagDefaultChild, t.tagChildren))

-- | @'updateSingleton' p f t@ replaces the binding at @p@ with @f@ of it.
updateSingleton :: Path -> (Maybe (d, t) -> Maybe (d, t)) -> Trie d t -> Trie d t
updateSingleton path f v = updateCont path v (updateRoot f)

-- ---------------------------------------------------------------------------
-- Union
-- ---------------------------------------------------------------------------

{- | A merger reconciles two bindings that collide at the same path, in a base
monad @m@.
-}
type Merger m d t = BwdPath -> (d, t) -> (d, t) -> m (d, t)

unionMaybeM :: (Applicative m) => (a -> a -> m a) -> Maybe a -> Maybe a -> m (Maybe a)
unionMaybeM _ Nothing Nothing = pure Nothing
unionMaybeM _ (Just r) Nothing = pure (Just r)
unionMaybeM _ Nothing (Just r) = pure (Just r)
unionMaybeM g (Just a) (Just b) = Just <$> g a b

unionNodeM :: (Monad m) => BwdPath -> Merger m d t -> Node d t -> Node d t -> m (Node d t)
unionNodeM prefixP m n1_ n2_ = do
  let Node nd1 nt1 = dropTagDefaultChild n1_
      Node nd2 nt2 = dropTagDefaultChild n2_
  mergedRoot <- unionMaybeM (m prefixP) (findRootNode (Node nd1 nt1)) (findRootNode (Node nd2 nt2))
  let (r, tr) = splitMaybe mergedRoot
      -- after dropTagDefaultChild, tagChildren has exactly the keys of dChildren
      node1children = Map.intersectionWith Node (nd1.dChildren) (nt1.tagChildren)
      node2children = Map.intersectionWith Node (nd2.dChildren) (nt2.tagChildren)
  combined <-
    Merge.mergeA
      Merge.preserveMissing
      Merge.preserveMissing
      (Merge.zipWithAMatched (\seg c1 c2 -> unionNodeM (prefixP <: seg) m c1 c2))
      node1children
      node2children
  let children' = Map.map (\(Node dd _) -> dd) combined
      tagChildren' = Map.map (\(Node _ tt) -> tt) combined
  pure (Node (DataNode r children') (TagNode tr Nothing tagChildren'))

-- | Monadic 'union'.
unionM :: (Monad m) => BwdPath -> Merger m d t -> Trie d t -> Trie d t -> m (Trie d t)
unionM prefixP m = unionMaybeM (unionNodeM prefixP m)

unionRootM :: (Monad m) => BwdPath -> Merger m d t -> Trie d t -> (d, t) -> m (Trie d t)
unionRootM _ _ Nothing v2 = pure (root v2)
unionRootM prefixP m (Just (Node d1 t1)) v2 = do
  merged <- unionMaybeM (m prefixP) (findRootNode (Node d1 t1)) (Just v2)
  let (r, tr) = splitMaybe merged
  pure (nonEmpty (Node (d1{dRoot = r}) (t1{tagRoot = tr})))

unionSingletonM :: (Monad m) => BwdPath -> Merger m d t -> Trie d t -> (Path, (d, t)) -> m (Trie d t)
unionSingletonM prefixP m v1 (path, v2) =
  updateContM path v1 (\v1' -> unionRootM (prefixP <@ path) m v1' v2)

unionSubtreeM :: (Monad m) => BwdPath -> Merger m d t -> Trie d t -> (Path, Trie d t) -> m (Trie d t)
unionSubtreeM prefixP m v1 (path, v2) =
  updateContM path v1 (\v1' -> unionM (prefixP <@ path) m v1' v2)

-- Pure wrappers, defined via the monadic ones with 'Identity'.

type PureMerger d t = BwdPath -> (d, t) -> (d, t) -> (d, t)

liftMerger :: PureMerger d t -> Merger Identity d t
liftMerger m p a b = Identity (m p a b)

{- | @'union' prefix merger t1 t2@ merges two tries, calling @merger@ on
collisions.  The @prefix@ is prepended to any path sent to @merger@; use
@'Emp'@ for none.
-}
union :: BwdPath -> PureMerger d t -> Trie d t -> Trie d t -> Trie d t
union prefixP m t1 t2 = runIdentity (unionM prefixP (liftMerger m) t1 t2)

unionSubtree :: BwdPath -> PureMerger d t -> Trie d t -> (Path, Trie d t) -> Trie d t
unionSubtree prefixP m t pv = runIdentity (unionSubtreeM prefixP (liftMerger m) t pv)

unionSingleton :: BwdPath -> PureMerger d t -> Trie d t -> (Path, (d, t)) -> Trie d t
unionSingleton prefixP m t pv = runIdentity (unionSingletonM prefixP (liftMerger m) t pv)

unionRoot :: BwdPath -> PureMerger d t -> Trie d t -> (d, t) -> Trie d t
unionRoot prefixP m t v = runIdentity (unionRootM prefixP (liftMerger m) t v)

-- ---------------------------------------------------------------------------
-- Detaching subtrees
-- ---------------------------------------------------------------------------

-- The OCaml uses a mutable ref to smuggle the detached value out of the update
-- continuation, which runs exactly once.  Here that is a @'Writer' ('First' a)@.
applyAndUpdateCont :: forall a d t. Path -> Trie d t -> (Trie d t -> (a, Trie d t)) -> (a, Trie d t)
applyAndUpdateCont path t k =
  case t of
    Nothing -> let (a, t') = k empty in (a, prefix path t')
    Just n ->
      let step :: Trie d t -> Writer (First a) (Trie d t)
          step tr = let (a, tr') = k tr in writer (tr', First (Just a))
          (t', First mAns) = runWriter (updateNodeContM path n step)
       in (fromMaybe (error "Yclept.Trie: detach continuation not run") mAns, t')

{- | @'detachSubtree' p t@ splits off the subtree at @p@; returns
@(subtree, remainder)@.
-}
detachSubtree :: Path -> Trie d t -> (Trie d t, Trie d t)
detachSubtree path t = applyAndUpdateCont path t (\tr -> (tr, empty))

-- | @'detachRoot' t@ splits off the binding at the root.
detachRoot :: Trie d t -> (Maybe (d, t), Trie d t)
detachRoot Nothing = (Nothing, empty)
detachRoot (Just (Node d t)) =
  ( findRootNode (Node d t)
  , mkTree (Nothing, d.dChildren) (Nothing, (t.tagDefaultChild, t.tagChildren))
  )

-- | @'detachSingleton' p t@ splits off the binding at @p@.
detachSingleton :: Path -> Trie d t -> (Maybe (d, t), Trie d t)
detachSingleton path t = applyAndUpdateCont path t detachRoot

-- ---------------------------------------------------------------------------
-- Mapping and filtering
-- ---------------------------------------------------------------------------

filterMapNode :: BwdPath -> (BwdPath -> (d1, t1) -> Maybe (d2, t2)) -> Node d1 t1 -> Trie d2 t2
filterMapNode prefixP f n =
  let (r, tr) = splitMaybe (findRootNode n >>= f prefixP)
      combined =
        Map.mapMaybeWithKey
          (\seg child -> filterMapNode (prefixP <: seg) f child)
          (getChildrenNode n)
      children' = Map.map (\(Node dd _) -> dd) combined
      tagChildren' = Map.map (\(Node _ tt) -> tt) combined
   in mkTree (r, children') (tr, (Nothing, tagChildren'))

{- | @'filterMapWithPath' prefix f t@ applies @f@ (which sees the path) to each
binding, keeping only the 'Just' results.  @prefix@ is prepended to the
paths sent to @f@; use @'Emp'@ for none.
-}
filterMapWithPath :: BwdPath -> (BwdPath -> (d1, t1) -> Maybe (d2, t2)) -> Trie d1 t1 -> Trie d2 t2
filterMapWithPath prefixP f v = v >>= filterMapNode prefixP f

-- | @'mapWithPath' prefix f t@ maps @f@ over every binding.
mapWithPath :: BwdPath -> (BwdPath -> (d1, t1) -> (d2, t2)) -> Trie d1 t1 -> Trie d2 t2
mapWithPath prefixP f = filterMapWithPath prefixP (\p x -> Just (f p x))

-- | @'filterWithPath' prefix f t@ keeps bindings for which @f@ returns 'True'.
filterWithPath :: BwdPath -> (BwdPath -> (d, t) -> Bool) -> Trie d t -> Trie d t
filterWithPath prefixP f =
  filterMapWithPath prefixP (\p x -> if f p x then Just x else Nothing)

-- ---------------------------------------------------------------------------
-- Iteration / conversion
-- ---------------------------------------------------------------------------

toListNode :: BwdPath -> Node d t -> [(BwdPath, (d, t))]
toListNode prefixP n =
  maybe [] (\r -> [(prefixP, r)]) (findRootNode n)
    ++ concatMap
      (\(seg, child) -> toListNode (prefixP <: seg) child)
      (Map.toAscList (getChildrenNode n))

{- | @'iter' prefix f t@ runs @f@ (which sees the path) on every binding, in
lexicographic order.
-}
iter :: (Applicative m) => BwdPath -> (BwdPath -> (d, t) -> m ()) -> Trie d t -> m ()
iter prefixP f = traverse_ (uncurry f) . toSeqWithBwdPaths prefixP

-- | Lexicographic traversal, backward paths.
toSeqWithBwdPaths :: BwdPath -> Trie d t -> [(BwdPath, (d, t))]
toSeqWithBwdPaths prefixP = maybe [] (toListNode prefixP)

-- | Lexicographic traversal, forward paths.
toSeq :: BwdPath -> Trie d t -> [(Path, (d, t))]
toSeq prefixP = map (\(p, v) -> (foldr (:) [] p, v)) . toSeqWithBwdPaths prefixP

-- | Lexicographic traversal, values only.
toSeqValues :: Trie d t -> [(d, t)]
toSeqValues = map snd . toSeqWithBwdPaths Emp

-- | Build a trie from a list, later bindings shadowing earlier ones.
ofSeq :: [(Path, (d, t))] -> Trie d t
ofSeq = ofSeqWithMerger Emp (\_ _ y -> y)

-- | Build a trie from a list, resolving collisions with @merger@.
ofSeqWithMerger :: BwdPath -> PureMerger d t -> [(Path, (d, t))] -> Trie d t
ofSeqWithMerger prefixP m = foldl' (unionSingleton prefixP m) empty

-- | Monadic 'ofSeqWithMerger'.
ofSeqWithMergerM :: (Monad m) => BwdPath -> Merger m d t -> [(Path, (d, t))] -> m (Trie d t)
ofSeqWithMergerM prefixP m = foldM (unionSingletonM prefixP m) empty

-- ---------------------------------------------------------------------------
-- Tags
-- ---------------------------------------------------------------------------

mapTagNode :: (t1 -> t2) -> TagNode t1 -> TagNode t2
mapTagNode f (TagNode r dc ch) =
  TagNode (fmap f r) (fmap f dc) (Map.map (mapTagNode f) ch)

-- | @'mapTag' f t@ applies @f@ to every tag, leaving data intact.
mapTag :: (t1 -> t2) -> Trie d t1 -> Trie d t2
mapTag _ Nothing = Nothing
mapTag f (Just (Node d t)) = Just (Node d (mapTagNode f t))

-- | @'retag' tag t@ resets every tag to @tag@ in @O(1)@.
retag :: t -> Trie d t' -> Trie d t
retag _ Nothing = Nothing
retag tg (Just (Node d _)) = nonEmpty (mkNode' d (Just tg))

-- | @'untag'@ is @'retag' ()@.
untag :: Trie d t -> Untagged d
untag = retag ()

-- | @'retagSubtree' path tag t@ resets tags within the subtree at @path@.
retagSubtree :: Path -> t -> Trie d t -> Trie d t
retagSubtree path tg = updateSubtree path (retag tg)

tagListNode :: TagNode t -> [t]
tagListNode (TagNode r dc ch) =
  maybeToList r ++ maybeToList dc ++ concatMap tagListNode (Map.elems ch)

-- | @'setOfTags' t@ returns the set of tags used in @t@.
setOfTags :: (Ord t) => Trie d t -> Set t
setOfTags Nothing = Set.empty
setOfTags (Just (Node _ t)) = Set.fromList (tagListNode t)
