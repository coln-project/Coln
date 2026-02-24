module Geolog.Elaborator.Diagnostics where

import Geolog.Diagnostician.CodeMeta

data ElaboratorCode
  = FailedConversion
  deriving (Eq, Ord)

elaboratorCodeTable :: [(ElaboratorCode, Int, CodeMeta)]
elaboratorCodeTable = [
  (FailedConversion, 0, CodeMeta Error Nothing)
  ]
