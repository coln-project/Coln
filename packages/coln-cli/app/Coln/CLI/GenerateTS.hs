-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.CLI.GenerateTS where

import Coln.Top
import Coln.CLI.Options

generateTS :: GenerateTSOptions -> IO ()
generateTS opts = do
  (_, realms) <- loadRealms opts.inputFile
  writeTS opts.outputDir realms
