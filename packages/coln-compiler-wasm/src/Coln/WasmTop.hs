-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.WasmTop () where

import Prelude hiding (span)
import Coln.FLIR.Value qualified as FLIR
import Coln.Top
import Coln.Diagnostics
import Coln.SIR.Realm qualified as SIR
import Coln.Common
import Coln.Util.JSString

import Data.IORef
import Data.Aeson qualified as AE
import Data.Aeson.Encoding qualified as AE
import Data.Map.Ordered qualified as OMap
import Diagnostician.HTML
import GHC.Wasm.Prim
import Lucid qualified as Lucid
import Prettyprinter.Render.Text
import Data.ByteString qualified as BS
import Data.Text.Lazy qualified as TL

data RealmProducts = RealmProducts
  { flir :: FLIR.Realm
  , flirPretty :: Text
  , tsModule :: Text
  }

instance AE.ToJSON RealmProducts where
  toJSON = panic "aesons behaving badly"
  toEncoding r =
    AE.pairs $
      mconcat
        [ AE.pair "flir" $ AE.toEncoding r.flir
        , AE.pair "flirPretty" $ AE.toEncoding r.flirPretty
        , AE.pair "tsModule" $ AE.toEncoding r.tsModule
        ]

getProducts :: Name -> SIR.Realm -> RealmProducts
getProducts x r = do
  let flirRealm = generateIr r
  RealmProducts
    { flir = flirRealm
    , flirPretty = irPretty flirRealm
    , tsModule = generateTs x r
    }

data Annotation = Annotation
  { span :: Maybe (Int, Int)
  , code :: Text
  , message :: Text
  }

instance AE.ToJSON Annotation where
  toJSON = panic "aesons behaving badly"
  toEncoding d =
    AE.pairs $
      mconcat
        [ AE.pair "span" $ AE.toEncoding d.span
        , AE.pair "code" $ AE.toEncoding d.code
        , AE.pair "message" $ AE.toEncoding d.message
        ]

getAnnotation :: Diagnostic ColnCode -> Annotation
getAnnotation d = do
  let span = case d.notes of
        [n] -> (<$> n.noteSourceLoc) $ \l -> (l.span.start, l.span.end)
        _ -> Nothing
  let code = renderStrict $ layoutCompact $ prtCode d.code
  let message = renderStrict $ layoutCompact $ d.summary
  Annotation span code message
  
data RenderedDiagnostic = RenderedDiagnostic
  { html :: TL.Text
  , annotation :: Annotation
  }

instance AE.ToJSON RenderedDiagnostic where
  toJSON = panic "aesons behaving badly"
  toEncoding d =
    AE.pairs $
      mconcat
        [ AE.pair "html" $ AE.toEncoding d.html
        , AE.pair "annotation" $ AE.toEncoding d.annotation
        ]

renderDiagnostic :: Diagnostic ColnCode -> RenderedDiagnostic
renderDiagnostic d = do
  let html = Lucid.renderText $ diagnosticToHtml d
  RenderedDiagnostic html (getAnnotation d)

data CompileResult = CompileResult
  { products :: OMap Name RealmProducts
  , diagnostics :: [RenderedDiagnostic]
  }

nameToKey :: Name -> AE.Encoding' AE.Key
nameToKey = AE.text . renderText . dpretty

instance AE.ToJSON CompileResult where
  toJSON = panic "aesons behaving badly"
  toEncoding d =
    AE.pairs $
      mconcat
        [ AE.pair "products" $ AE.dict nameToKey AE.toEncoding (\f accInit -> foldr (\(x, v) acc -> f x v acc) accInit . OMap.assocs) d.products
        , AE.pair "diagnostics" $ AE.toEncoding d.diagnostics
        ]

fullPipeline :: Text -> IO CompileResult
fullPipeline src = do
  let f = newFile "<input>" src
  dRef <- newIORef []
  let rep = pureReporter dRef
  realms <- loadRealmsFromFile rep f
  let products = OMap.fromList [(x, getProducts x r) | (x, r) <- OMap.assocs realms ]
  diagnostics <- (fmap renderDiagnostic) <$> readIORef dRef
  pure $ CompileResult products diagnostics

jsCompile :: JSString -> IO JSString
jsCompile src = do
  res <- fullPipeline (textFromJSString src)
  pure $ byteStringToJSString $ BS.toStrict $ AE.encode res
  
foreign export javascript "compile" jsCompile :: JSString -> IO JSString
