-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module FNotation.Tokens where

import Data.Text (Text)
import Diagnostician
import FNotation.Kinds
import FNotation.Names
import Prettyprinter

-- * Tokens

--------------------------------------------------------------------------------

data TokenValue = VEmpty | VName Name | VInt Int | VString Text
  deriving (Eq, Show)

{- | A @Token@ consists of a kind along with an attached value and a source code
location.

We split the kind and the attached value so that we can store a set of token
kinds as a data structure; otherwise the only way to classify tokens would be
functions.
-}
data Token = Token
  { kind :: Kind
  , value :: TokenValue
  , span :: Span
  }
  deriving (Eq)

instance DPretty Token where
  dpretty (Token k v s) = dpretty k <> pv <+> "(" <> dpretty s <> ")"
   where
    pv = case v of
      VEmpty -> mempty
      VName x -> " " <> dpretty x
      VInt i -> " " <> pretty i
      VString i -> " " <> pretty i

-- * Token classes (used for error messages)

--------------------------------------------------------------------------------

data Class
  = CSpecific Kind
  | CExprStart
  | CTupleMark

instance DPretty Class where
  dpretty = \case
    CSpecific k -> dpretty k
    CExprStart -> "a token that can start an expression"
    CTupleMark ->
      "a token that can follow an element of a tuple, e.g. ',' or ']'"
