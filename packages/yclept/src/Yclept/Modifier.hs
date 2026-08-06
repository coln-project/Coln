-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT
{-# LANGUAGE OverloadedRecordDot #-}

{- | The modifier engine, a port of the OCaml @Modifier@ / @ModifierSigs@.

The OCaml engine performs three /overridable/ algebraic effects —
@not_found@, @shadow@, and @hook@.  Here those become the fields of an
explicit __bundle of handlers__, 'Handlers', which is polymorphic over a base
monad @m@.  Every engine function ('modify', 'union', …) takes a bundle and
runs in @m@.

Because the OCaml effect handlers were installed dynamically and could be
overridden per-region (via @run@ / @try_with@), the ceremony around them
dissolves here: to \"override\" a handler you simply pass a different bundle
(e.g. @hs { shadow = … }@).  The @Perform@ / @try_with@ machinery therefore
lives at the scope layer ("Yclept.Scope"), which threads the current
bundle through a reader environment.

The four OCaml @Param@ types become the type variables @d@ (data), @t@
(tag), @h@ (hook), and @c@ (context); the @Make(Param)@ functor is just this
parametric polymorphism.
-}
module Yclept.Modifier (
  -- * The bundle of handlers
  Handlers (..),
  silence,

  -- * The engine
  modify,

  -- * Re-exposed union operations (using the @shadow@ handler)
  union,
  unionSubtree,
  unionSingleton,
  unionRoot,
) where

import Control.Monad (foldM)

import Yclept.Bwd ((<@))
import Yclept.Language (Language (..))
import Yclept.Trie (BwdPath, Trie)
import Yclept.Trie qualified as Trie

{- | A bundle of handlers for the three overridable modifier effects, in a base
monad @m@.  @d@\/@t@ are the data\/tag, @h@ the hook label, @c@ the context.
-}
data Handlers m d t h c = Handlers
  { notFound :: Maybe c -> BwdPath -> m ()
  {- ^ Called when a modifier expected at least one binding under a prefix but
  found none.
  -}
  , shadow :: Maybe c -> BwdPath -> (d, t) -> (d, t) -> m (d, t)
  {- ^ Called to reconcile two bindings @x@ (earlier) and @y@ (later) at the
  same path during a union.
  -}
  , hook :: Maybe c -> BwdPath -> h -> Trie d t -> m (Trie d t)
  -- ^ Called to run a custom hook on a subtree.
  }

{- | The handlers that silence every effect: @not_found@ does nothing, @shadow@
keeps the later binding, @hook@ returns its input unchanged.
-}
silence :: (Applicative m) => Handlers m d t h c
silence =
  Handlers
    { notFound = \_ _ -> pure ()
    , shadow = \_ _ _ y -> pure y
    , hook = \_ _ _ t -> pure t
    }

{- | @'modify' hs ctx prefix m t@ runs the modifier @m@ on the trie @t@, using
the handler bundle @hs@ for effects.  @ctx@ is the context passed to
handlers; @prefix@ is prepended to any path reported to them (use @'Emp'@
for none).
-}
modify :: (Monad m) => Handlers m d t h c -> Maybe c -> BwdPath -> Language h -> Trie d t -> m (Trie d t)
modify hs ctx = go
 where
  go prefix m t =
    case m of
      MAssertNonempty -> do
        if Trie.isEmpty t then hs.notFound ctx prefix else pure ()
        pure t
      MIn p m' ->
        Trie.updateSubtreeM p (go (prefix <@ p) m') t
      MRenaming p1 p2 ->
        let (sub, remaining) = Trie.detachSubtree p1 t
         in pure (Trie.updateSubtree p2 (const sub) remaining)
      MSeq ms ->
        foldM (\acc m' -> go prefix m' acc) t ms
      MUnion ms ->
        foldM
          ( \ts m' -> do
              ti <- go prefix m' t
              union hs ctx prefix ts ti
          )
          Trie.empty
          ms
      MHook h ->
        hs.hook ctx prefix h t

-- | Re-exposed 'Trie.union' whose merger is the @shadow@ handler.
union :: (Monad m) => Handlers m d t h c -> Maybe c -> BwdPath -> Trie d t -> Trie d t -> m (Trie d t)
union hs ctx prefix = Trie.unionM prefix (hs.shadow ctx)

-- | Re-exposed 'Trie.unionSubtree' using the @shadow@ handler.
unionSubtree :: (Monad m) => Handlers m d t h c -> Maybe c -> BwdPath -> Trie d t -> (Trie.Path, Trie d t) -> m (Trie d t)
unionSubtree hs ctx prefix = Trie.unionSubtreeM prefix (hs.shadow ctx)

-- | Re-exposed 'Trie.unionSingleton' using the @shadow@ handler.
unionSingleton :: (Monad m) => Handlers m d t h c -> Maybe c -> BwdPath -> Trie d t -> (Trie.Path, (d, t)) -> m (Trie d t)
unionSingleton hs ctx prefix = Trie.unionSingletonM prefix (hs.shadow ctx)

-- | Re-exposed 'Trie.unionRoot' using the @shadow@ handler.
unionRoot :: (Monad m) => Handlers m d t h c -> Maybe c -> BwdPath -> Trie d t -> (d, t) -> m (Trie d t)
unionRoot hs ctx prefix = Trie.unionRootM prefix (hs.shadow ctx)
