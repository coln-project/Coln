-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.CLI.Check where

import Coln.CLI.Common
import Coln.CLI.Options

check :: CheckOptions -> IO ()
check opts = do
  _ <- loadFile opts.inputFile
  pure ()
