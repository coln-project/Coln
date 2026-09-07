-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

{- | The modifier language, a port of the OCaml @Language@ / @LanguageSigs@.

A @'Language' h@ value is an abstract syntax tree describing how to transform
a trie; it is executed by "Yclept.Modifier".  Build values with the smart
constructors ('all', 'only', 'renaming', …); the raw constructors are also
exported so the engine (and clients who wish to) can pattern-match.

Intended to be imported qualified:

> import qualified Yclept.Language as Language
-}
module Yclept.Language (
  Language (..),

  -- * Builders
  all,
  id,
  only,
  in_,
  none,
  except,
  renaming,
  seq,
  union,
  hook,

  -- * Debugging
  dump,
) where

import Data.Functor.Classes (Eq1 (..))
import Data.List (intercalate)
import Data.Text qualified as Text
import Prelude hiding (all, id, seq)

import Yclept.Trie (Path)

-- | The abstract type of modifiers, parametrised by the type of hook labels.
data Language h
  = MAssertNonempty
  | MIn Path (Language h)
  | MRenaming Path Path
  | MSeq [Language h]
  | MUnion [Language h]
  | MHook h
  deriving (Eq, Show, Functor, Foldable, Traversable)

{- | Structural equality with a supplied hook comparison (the OCaml @equal@).
The ordinary 'Eq' instance is @'liftEq' '=='@.
-}
instance Eq1 Language where
  liftEq _ MAssertNonempty MAssertNonempty = True
  liftEq eqh (MIn p1 m1) (MIn p2 m2) = p1 == p2 && liftEq eqh m1 m2
  liftEq _ (MRenaming a1 b1) (MRenaming a2 b2) = a1 == a2 && b1 == b2
  liftEq eqh (MSeq xs) (MSeq ys) = liftEq (liftEq eqh) xs ys
  liftEq eqh (MUnion xs) (MUnion ys) = liftEq (liftEq eqh) xs ys
  liftEq eqh (MHook h1) (MHook h2) = eqh h1 h2
  liftEq _ _ _ = False

{- | Keep the content of the current tree, performing @not_found@ if it is
empty.  Equivalent to @'only' []@.
-}
all :: Language h
all = MAssertNonempty

{- | The identity modifier (@'seq' []@); like 'all' but without the emptiness
check.
-}
id :: Language h
id = seq []

{- | Keep the subtree rooted at @path@, dropping everything else; performs
@not_found@ if that subtree is empty.
-}
only :: Path -> Language h
only p = MSeq [MIn p MAssertNonempty, MRenaming p [], MRenaming [] p]

-- | Run a modifier on the subtree rooted at @path@, leaving the rest intact.
in_ :: Path -> Language h -> Language h
in_ = MIn

-- | Drop everything, performing @not_found@ if the tree was already empty.
none :: Language h
none = MSeq [MAssertNonempty, MUnion []]

-- | Drop the subtree rooted at @p@ (@'in_' p 'none'@).
except :: Path -> Language h
except p = in_ p none

{- | Relocate the subtree at @path@ to @path'@, dropping any existing bindings
under @path'@; performs @not_found@ if the source subtree is empty.
-}
renaming :: Path -> Path -> Language h
renaming p p' = MSeq [MIn p MAssertNonempty, MRenaming p p']

-- | Run the modifiers in order (@'seq' []@ is 'id').
seq :: [Language h] -> Language h
seq = MSeq

-- | Union of the results of the given modifiers; collisions trigger @shadow@.
union :: [Language h] -> Language h
union = MUnion

-- | Apply the hook labelled @h@ to the whole tree (performs the @hook@ effect).
hook :: h -> Language h
hook = MHook

-- | Dump the internal representation for debugging, given a printer for hooks.
dump :: (h -> String) -> Language h -> String
dump dumpHook = go
 where
  go MAssertNonempty = "assert-nonempty"
  go (MIn p m) = "in(" ++ dumpPath p ++ "; " ++ go m ++ ")"
  go (MRenaming p1 p2) = "renaming(" ++ dumpPath p1 ++ "; " ++ dumpPath p2 ++ ")"
  go (MSeq ms) = "seq(" ++ intercalate "; " (map go ms) ++ ")"
  go (MUnion ms) = "union(" ++ intercalate "; " (map go ms) ++ ")"
  go (MHook h) = "hook(" ++ dumpHook h ++ ")"

dumpPath :: Path -> String
dumpPath [] = "root"
dumpPath segs = "path(" ++ intercalate ", " (map (show . Text.unpack) segs) ++ ")"
