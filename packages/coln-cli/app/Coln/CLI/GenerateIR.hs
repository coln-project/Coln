-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.CLI.GenerateIR where

import Coln.Backend.Lower
import Coln.CLI.Common
import Coln.CLI.Options

generateIR :: GenerateIROptions -> IO ()
generateIR opts = do
  ge <- loadFile opts.inputFile
  writeIRFor ge opts.outputDir
  pure ()
