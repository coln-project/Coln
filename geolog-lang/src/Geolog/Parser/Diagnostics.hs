module Geolog.Parser.Diagnostics where

import Geolog.Diagnostician.CodeMeta

data ParserCode
  = UnexpectedToken
  | DefaultedPrec
  | IncompatiblePrecedences
  deriving (Eq, Ord)

parserCodeTable :: [(ParserCode, Int, CodeMeta)]
parserCodeTable =
  [ (UnexpectedToken, 0, CodeMeta Error Nothing)
  , (DefaultedPrec, 1, CodeMeta Warning Nothing)
  , (IncompatiblePrecedences, 2, CodeMeta Error Nothing)
  ]
