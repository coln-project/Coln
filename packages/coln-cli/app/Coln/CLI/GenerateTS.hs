module Coln.CLI.GenerateTS where

import Coln.CLI.Common
import Coln.CLI.Options

generateTS :: GenerateTSOptions -> IO ()
generateTS opts = do
  ge <- loadFile opts.inputFile
  pure ()
