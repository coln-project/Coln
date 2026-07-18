-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Lib () where

import Coln.Backend.IR qualified as I
import Coln.Backend.Lower
import Coln.Core.Globals
import Coln.Diagnostics
import Coln.Frontend.Parser
import Control.Monad
import Data.Aeson.Text qualified as Aeson
import Data.Foldable
import Data.IORef
import Data.Map.Ordered qualified as OMap
import Data.Text (Text)
import Data.Text qualified as T
import Data.Text.Lazy qualified as TL
import Diagnostician
import Diagnostician.HTML (diagnosticToHtml)
import Foreign.StablePtr
import GHC.Wasm.Prim
import Lucid qualified
import Prettyprinter
import Prettyprinter.Render.Text qualified as Text

data CompileResult = CompileResult
  { ir :: [I.FlatRealm]
  , diagnostics :: [Diagnostic ColnCode]
  }
foreign export javascript "freeCompileResult" freeStablePtr :: StablePtr CompileResult -> IO ()

compile :: JSString -> IO (StablePtr CompileResult)
compile src = do
  ref <- newIORef []
  globals <- topFromText (pureReporter ref) (newFile "<wasm>" $ textFromJSString src)
  let ir = map (uncurry lowerRealm) $ OMap.assocs globals.realms
  diagnostics <- reverse <$> readIORef ref
  newStablePtr CompileResult{ir, diagnostics}
foreign export javascript "compile" compile :: JSString -> IO (StablePtr CompileResult)

getDiagnostics :: Bool -> StablePtr CompileResult -> IO JSVal
getDiagnostics asHtml = jsStringArray . map (textToJSString . TL.toStrict . render) . (.diagnostics) <=< deRefStablePtr
 where
  render =
    if asHtml
      then Lucid.renderText . diagnosticToHtml
      else Text.renderLazy . layoutPretty defaultLayoutOptions . dpretty
foreign export javascript "getDiagnostics" getDiagnostics :: Bool -> StablePtr CompileResult -> IO JSVal

prettyIr :: StablePtr CompileResult -> IO JSVal
prettyIr = jsStringArray . map (textToJSString . render . dpretty) . (.ir) <=< deRefStablePtr
 where
  render = Text.renderStrict . layoutPretty defaultLayoutOptions
foreign export javascript "prettyIr" prettyIr :: StablePtr CompileResult -> IO JSVal

irToJson :: StablePtr CompileResult -> IO JSString
irToJson = fmap (textToJSString . TL.toStrict . Aeson.encodeToLazyText . (.ir)) . deRefStablePtr
foreign export javascript "irToJson" irToJson :: StablePtr CompileResult -> IO JSString

textToJSString :: Text -> JSString
textToJSString = toJSString . T.unpack
textFromJSString :: JSString -> Text
textFromJSString = T.pack . fromJSString

foreign import javascript unsafe "[]" js_new_array :: IO JSVal
foreign import javascript unsafe "$1.push($2)" js_push_string :: JSVal -> JSString -> IO ()
jsStringArray :: [JSString] -> IO JSVal
jsStringArray ss = do
  arr <- js_new_array
  for_ ss $ js_push_string arr
  pure arr
