-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

{- | Name pattern combinators for hierarchical names — based on the OCaml
  @yuujinchou@ library.

This is a documentation landing module: it deliberately exports nothing,
because the submodules share many names (@empty@, @union@, @singleton@, …)
and are designed to be imported /qualified/, like "Data.Map":

> import qualified Yclept.Bwd as Bwd
> import qualified Yclept.Trie as Trie
> import qualified Yclept.Trie.Untagged as UntaggedTrie
> import qualified Yclept.Language as Language
> import qualified Yclept.Modifier as Modifier
> import qualified Yclept.Scope as Scope

The pieces:

* "Yclept.Bwd": backward (snoc) lists, used for path prefixes.
* "Yclept.Trie" and "Yclept.Trie.Untagged": the trie data structure.
* "Yclept.Language": the modifier DSL.
* "Yclept.Modifier": the modifier engine and its 'Yclept.Modifier.Handlers' bundle.
* "Yclept.Scope": the lexical-scope engine.
-}
module Yclept () where
