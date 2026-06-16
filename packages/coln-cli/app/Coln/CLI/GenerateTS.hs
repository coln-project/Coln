module Coln.CLI.GenerateTS where

import Coln.CLI.Common
import Coln.CLI.Options
import Coln.Backend.TypeScript.Generate

generateTS :: GenerateTSOptions -> IO ()
generateTS opts = do
  ge <- loadFile opts.inputFile
  generate ge opts.outputDir
  pure ()
