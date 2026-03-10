module Geolog.Lexer.Diagnostics where

import Geolog.Diagnostician.CodeMeta

data LexerCode
  = UnexpectedCharacter
  | UncontinuedQualifiedName
  deriving (Eq, Ord)

lexerCodeTable :: [(LexerCode, Int, CodeMeta)]
lexerCodeTable =
  [ (UnexpectedCharacter, 0, CodeMeta Error Nothing),
    (UncontinuedQualifiedName, 1, CodeMeta Error Nothing)
  ]
