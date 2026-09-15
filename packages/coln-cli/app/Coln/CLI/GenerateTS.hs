-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.CLI.GenerateTS where

import Control.Monad (forM_)
import Data.Map.Ordered qualified as OMap
import Data.Text.IO qualified as TIO
import System.FilePath ((</>))

import Coln.Top
import Coln.Common
import Coln.CLI.Common
import Coln.CLI.Options

generateTS :: GenerateTSOptions -> IO ()
generateTS opts = do
  globals <- loadFile opts.inputFile
  let realms = lowerRealms globals
  forM_ (OMap.assocs realms) $ \(x, r) -> do
    let fn = opts.outputDir </> mangleToString x <> ".ts"
    TIO.writeFile fn (generateTs x r)
