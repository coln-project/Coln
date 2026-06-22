module Coln.CLI.GenerateTS where

import Coln.Backend.TypeScript.Generate
import Coln.CLI.Common
import Coln.CLI.Options

generateTS :: GenerateTSOptions -> IO ()
generateTS opts = do
  ge <- loadFile opts.inputFile
  generate ge opts.outputDir
  pure ()
