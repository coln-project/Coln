-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.CLI.GenerateIR where

import Coln.Top
import Coln.CLI.Options

generateIR :: GenerateIROptions -> IO ()
generateIR opts = do
  (rep, realms) <- loadRealms opts.inputFile
  writeFLIR opts.outputDir rep realms
