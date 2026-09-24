-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.CLI.GenerateIR where

import Control.Monad (forM_)
import Data.Aeson qualified as AE
import Data.Map.Ordered qualified as OMap
import Data.Text.IO qualified as TIO
import System.FilePath ((</>))

import Coln.CLI.Common
import Coln.CLI.Options
import Coln.Common
import Coln.Top

generateIR :: GenerateIROptions -> IO ()
generateIR opts = do
  globals <- loadFile opts.inputFile
  let realms = lowerRealms globals
  forM_ (OMap.assocs realms) $ \(x, r) -> do
    let flir = generateIr r
    let fnJson = opts.outputDir </> mangleToString x <> ".json"
    AE.encodeFile fnJson flir
    let fnPretty = opts.outputDir </> mangleToString x <> ".pretty"
    TIO.writeFile fnPretty (irPretty flir)
