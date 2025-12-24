module Geolog.Token where

import Geolog.Common
import Prettyprinter

data Kind
  = AIdent -- alphanumerical identifier
  | AKeyword -- alphanumerical keyword
  | SIdent -- symbolic identifier
  | SKeyword -- symbolic keyword
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

data Class
  = CSpecific Kind
  | CExprStart

instance Pretty Class where
  pretty = \case
    CSpecific k -> pretty k
    CExprStart -> "a token that can start an expression"

data TokenValue = VEmpty | VName Name | VInt Int
  deriving (Eq, Show)

data Token = Token
  { tokenKind :: Kind,
    tokenValue :: TokenValue,
    tokenSpan :: Span
  }
  deriving (Eq)

instance Pretty Token where
  pretty (Token k v s) = pretty k <> pv <+> "(" <> pretty s <> ")"
    where
      pv = case v of
        VEmpty -> mempty
        VName x -> " " <> pretty x
        VInt i -> " " <> pretty i
