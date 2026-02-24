module Geolog.Parser.Diagnostics where

import Geolog.Diagnostician.CodeMeta

data Code
  = UnexpectedToken
  | DefaultedPrec
  | IncompatiblePrecedences
  deriving (Eq, Ord)

table :: [(Code, Int, CodeMeta)]
table = [
  (UnexpectedToken, 0, CodeMeta Error Nothing),
  (DefaultedPrec, 1, CodeMeta Warning Nothing),
  (IncompatiblePrecedences, 2, CodeMeta Error Nothing)
  ]
