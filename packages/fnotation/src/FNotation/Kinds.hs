-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module FNotation.Kinds where

import Diagnostician (DPretty (..))
import Prettyprinter

data Kind
  = -- | alphanumerical identifier
    AIdent
  | -- | alphanumerical keyword
    AKeyword
  | -- | symbolic identifier
    SIdent
  | -- | symbolic keyword
    SKeyword
  | Decl
  | Modifier
  | Block
  | End
  | Tag
  | Field
  | -- | No-space field, like the `x` in `f.x`
    FieldImmediate
  | Mode
  | Int
  | String
  | LParen
  | RParen
  | LBrack
  | RBrack
  | LCurly
  | RCurly
  | Comma
  | Semicolon
  | Nl
  | Eof
  | Error
  deriving (Eq, Show)

instance DPretty Kind where
  dpretty k = pretty (show k)
