module Geolog.Elaborator.Diagnostics where

import Geolog.Diagnostician.CodeMeta

data Code
  = FailedConversion
  deriving (Eq, Ord)

table :: [(Code, Int, CodeMeta)]
table = [
  (FailedConversion, 0, CodeMeta Error Nothing)
  ]
