-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Test.FNotation.Golden (goldenTests) where

import Control.Exception
import Data.ByteString.Lazy qualified as LBS
import Data.Functor.Contravariant (contramap)
import Data.Map (Map)
import Data.Map qualified as Map
import Data.Text.IO.Utf8 qualified as T
import Data.Text.Lazy qualified as TL
import Data.Text.Lazy.Encoding qualified as TLE
import Data.Vector qualified as V
import Diagnostician
import FNotation
import Prettyprinter
import Prettyprinter.Render.Text
import System.FilePath (replaceExtension, takeBaseName)
import System.IO
import System.IO.Temp (withSystemTempFile)
import Test.FNotation.Common
import Test.Tasty (TestTree, testGroup)
import Test.Tasty.Golden (findByExtension, goldenVsString)
import Prelude hiding (lex, read)

render :: DDoc -> LBS.ByteString
render = TLE.encodeUtf8 . renderLazy . layoutPretty defaultLayoutOptions

readToPretty :: FilePath -> IO LBS.ByteString
readToPretty fp = do
  src <- T.readFile fp
  let f = newFile fp src
  withSystemTempFile "reporter-output" $ \path h -> do
    let r = fileReporter h
    try @SomeException (lex lexConfig (contramap LexerCode r) f) >>= \case
      Left err -> pure $ TLE.encodeUtf8 $ "lex error:\n" <> TL.show err
      Right tokens -> do
        try @SomeException (read readConfig (contramap ReaderCode r) f tokens) >>= \case
          Left err -> pure $ TLE.encodeUtf8 $ "read error:\n" <> TL.show err
          Right ns -> do
            hFlush h
            hClose h
            msgs <- T.readFile path
            pure $
              render $
                vsep
                  [ "-- tokens"
                  , vsep $ dpretty <$> V.toList tokens
                  , ""
                  , "-- notation"
                  , vsep $ dpretty <$> ns
                  , ""
                  , "-- pretty"
                  , vsep $ dprettyWithConfigs readConfig lexConfig <$> ns
                  , ""
                  , "-- messages"
                  , pretty $ msgs
                  ]

goldenTests :: IO TestTree
goldenTests = do
  ntnFiles <- findByExtension [".ntn"] "."
  return $
    testGroup
      "Golden tests"
      [ goldenVsString (takeBaseName ntnFile) outputFile (readToPretty ntnFile)
      | ntnFile <- ntnFiles
      , let outputFile = replaceExtension ntnFile ".output"
      ]
