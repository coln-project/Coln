module Geolog.Notation where

import FlatParse.Common.Position
import Geolog.Common
import Lens.Micro.TH

data Token
  = IDENT Name
  | KEYWORD Name
  | DECL Name
  | BLOCK Name
  | END
  | OP Name Prec
  | KEYWORD_OP Name Prec
  | INT Int
  | LPAREN
  | RPAREN
  | LBRACK
  | RBRACK
  | LCURLY
  | RCURLY
  | COMMA
  | SEMICOLON
  | NL
  | TAG Name
  | FIELD Name
  | EOF
  | ERROR

data Prec = LAssoc Int | NonAssoc Int | RAssoc Int

data Ntn
  = App1 Span Ntn Ntn
  | App2 Span Ntn Ntn Ntn
  | Block Span Name (Maybe Ntn) (Bwd Ntn)
  | Decl Span Name Ntn
  | Error Span

spanOf :: Ntn -> Span
spanOf (App1 s _ _) = s
spanOf (App2 s _ _ _) = s
spanOf (Block s _ _ _) = s
spanOf (Decl s _ _) = s
spanOf (Error s) = s
