module Coln.CLI.GenerateIR where

import Coln.CLI.Common
import Coln.CLI.Options
import Coln.Backend.Lower

generateIR :: GenerateIROptions -> IO ()
generateIR opts = do
  ge <- loadFile opts.inputFile
  writeIRFor ge opts.outputDir
  pure ()
