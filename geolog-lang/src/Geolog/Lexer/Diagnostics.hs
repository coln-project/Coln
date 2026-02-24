module Geolog.Lexer.Diagnostics where

import Geolog.Diagnostician.CodeMeta

data Code
  = UnexpectedCharacter
  | UncontinuedQualifiedName
  deriving (Eq, Ord)

table :: [(Code, Int, CodeMeta)]
table = [
  (UnexpectedCharacter, 0, CodeMeta Error Nothing),
  (UncontinuedQualifiedName, 1, CodeMeta Error Nothing)
  ]
