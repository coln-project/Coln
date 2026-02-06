module Geolog.Token where

import Geolog.Common
import Prettyprinter

-- Token kinds
--------------------------------------------------------------------------------

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
  | Block
  | End
  | Tag
  | Field
  | Int
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

instance Pretty Kind where
  pretty k = pretty (show k)

-- Tokens
--------------------------------------------------------------------------------

data TokenValue = VEmpty | VName Name | VQName QName | VInt Int
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

instance Pretty Token where
  pretty (Token k v s) = pretty k <> pv <+> "(" <> pretty s <> ")"
   where
    pv = case v of
      VEmpty -> mempty
      VName x -> " " <> pretty x
      VQName x -> " " <> pretty x
      VInt i -> " " <> pretty i

-- Token classes (used for error messages)
--------------------------------------------------------------------------------

data Class
  = CSpecific Kind
  | CExprStart
  | CTupleMark

instance Pretty Class where
  pretty = \case
    CSpecific k -> pretty k
    CExprStart -> "a token that can start an expression"
    CTupleMark ->
      "a token that can follow an element of a tuple, e.g. ',' or ']'"
