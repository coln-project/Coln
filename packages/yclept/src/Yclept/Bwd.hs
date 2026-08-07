-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

{- | Backward (snoc) lists, a port of the OCaml @bwd@ library.

A backward list grows at the /right/ end: @'Emp' ':<' \"x\" ':<' \"y\"@
represents the sequence @x, y@ and 'toList' yields @[\"x\", \"y\"]@.
-}
module Yclept.Bwd (
  Bwd (..),
  (#<),
  (<:),
  (<@),
  toList,
  length,
) where

import Data.Foldable (length, toList)
import Prelude hiding (length)

-- | A backward list.  The constructor ':<' is snoc: @xs ':<' x@.
data Bwd a
  = Emp
  | Bwd a :< a
  deriving (Eq, Ord, Show, Functor, Foldable, Traversable)

infixl 5 :<
infixl 5 #<
infixl 5 <:
infixl 5 <@

-- | Snoc a single element (OCaml @( #< )@).
(#<) :: Bwd a -> a -> Bwd a
(#<) = (:<)

{- | Snoc a single element (OCaml @( <: )@); an alias of '#<' kept for parity
with the OCaml @bwd@ API.
-}
(<:) :: Bwd a -> a -> Bwd a
(<:) = (:<)

{- | Append a forward list onto the right end of a backward list
(OCaml @( <\@ )@).
-}
(<@) :: Bwd a -> [a] -> Bwd a
(<@) = foldl (:<)
