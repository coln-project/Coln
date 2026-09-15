-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Main (main) where

import Coln.Diagnostics (ColnCode)
import Coln.Top (generateTs, loadRealmsFromFile)
import Data.IORef (newIORef, readIORef)
import Data.Map.Ordered qualified as OMap
import Data.Text qualified as Text
import Data.Text.IO qualified as Text
import Diagnostician (Diagnostic, newFile, pureReporter)

main :: IO ()
main = do
  source <- Text.readFile "test/golden/graph.coln"
  diagnosticRef <- newIORef ([] :: [Diagnostic ColnCode])
  realms <- loadRealmsFromFile (pureReporter diagnosticRef) $ newFile "graph.coln" source
  diagnostics <- readIORef diagnosticRef
  unlessNull diagnostics "graph compilation returned diagnostics"
  generated <- case OMap.assocs realms of
    [realm] -> pure $ uncurry generateTs realm
    _ -> fail "expected one graph realm"
  require "import type * as Runtime from \"@coln-project/runtime\";" generated
  require "export function createRealm(runtime: typeof Runtime)" generated
  require "constructor(store: Runtime.ManagedStore)" generated
  reject "import * as runtime" generated
  reject "new runtime.ManagedStore" generated

require :: Text.Text -> Text.Text -> IO ()
require expected actual
  | expected `Text.isInfixOf` actual = pure ()
  | otherwise = fail $ "missing generated output: " <> Text.unpack expected

reject :: Text.Text -> Text.Text -> IO ()
reject unexpected actual
  | unexpected `Text.isInfixOf` actual = fail $ "unexpected generated output: " <> Text.unpack unexpected
  | otherwise = pure ()

unlessNull :: [a] -> String -> IO ()
unlessNull [] _ = pure ()
unlessNull _ message = fail message
