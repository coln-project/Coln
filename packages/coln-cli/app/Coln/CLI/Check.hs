module Coln.CLI.Check where

import Coln.CLI.Common
import Coln.CLI.Options

check :: CheckOptions -> IO ()
check opts = do
  _ <- loadFile opts.inputFile
  pure ()
