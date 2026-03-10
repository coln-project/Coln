module Geolog.Diagnostician.CodeMeta where

import Data.Text

data Severity = Debug | Info | Warning | Error

data CodeMeta = CodeMeta
  { severity :: Severity
  , about :: Maybe Text
  }
