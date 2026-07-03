-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Lib () where

import Coln.Backend.IR qualified as I
import Coln.Backend.Lower
import Coln.Core.Globals
import Coln.Diagnostics
import Coln.Frontend.Driver
import Data.Aeson.Text qualified as Aeson
import Data.IORef
import Data.Map.Ordered qualified as OMap
import Data.Text (Text)
import Data.Text.Lazy qualified as TL
import Diagnostician
import Diagnostician.HTML (diagnosticToHtml)
import Foreign.StablePtr
import GHC.Wasm.Prim (JSString (..))
import Lucid qualified
import Prettyprinter
import Prettyprinter.Render.Text qualified as Text
import Wasm.Export

data CompileResult = CompileResult
  { ir :: [I.FlatRealm]
  , diagnostics :: [Diagnostic ColnCode]
  }

compile :: Text -> IO (StablePtr CompileResult)
compile src = do
  ref <- newIORef []
  globals <- topFromText (pureReporter ref) (newFile "<wasm>" src)
  let ir = map (uncurry lowerRealm) $ OMap.assocs globals.realms
  diagnostics <- reverse <$> readIORef ref
  newStablePtr CompileResult{ir, diagnostics}
$(exportDeclJS Async 'compile)

getDiagnostics :: Bool -> StablePtr CompileResult -> IO [Text]
getDiagnostics asHtml = fmap (map (TL.toStrict . render) . (.diagnostics)) . deRefStablePtr
 where
  render =
    if asHtml
      then Lucid.renderText . diagnosticToHtml
      else Text.renderLazy . layoutPretty defaultLayoutOptions . dpretty
$(exportDeclJS Async 'getDiagnostics)

prettyIr :: StablePtr CompileResult -> IO [Text]
prettyIr = fmap (map (render . dpretty) . (.ir)) . deRefStablePtr
 where
  render = Text.renderStrict . layoutPretty defaultLayoutOptions
$(exportDeclJS Async 'prettyIr)

irToJson :: StablePtr CompileResult -> IO Text
irToJson = fmap (TL.toStrict . Aeson.encodeToLazyText . (.ir)) . deRefStablePtr
$(exportDeclJS Async 'irToJson)
