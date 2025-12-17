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

data NtnKind
  = KApp1
  | KApp2 Prec
  | KBlock
  | KLeaf Token
  | KError

data Children
  = C0
  | C1 Ntn
  | C2 Ntn Ntn
  | C3 Ntn Ntn Ntn
  | CN (Bwd Ntn)

data Ntn = Ntn
  { ntnKind :: NtnKind,
    ntnLoc :: Span,
    ntnChildren :: Children
  }

makeFields ''Ntn

pattern Ident :: Name -> Ntn
pattern Ident n <- Ntn (KLeaf (IDENT n)) _ C0

pattern App1 :: Ntn -> Ntn -> Ntn
pattern App1 f x <- Ntn KApp1 _ (C2 f x)
