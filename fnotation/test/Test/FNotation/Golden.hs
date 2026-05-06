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
import Prelude hiding (lex)

render :: DDoc -> LBS.ByteString
render = TLE.encodeUtf8 . renderLazy . layoutPretty defaultLayoutOptions

data TestCode = LexerCode LexerCode | ParserCode ParserCode
  deriving (Eq, Ord)

codeTable :: Map TestCode CodeMeta
codeTable =
  mconcat
    [ promoteCodeTable lexerCodeTable LexerCode 0
    , promoteCodeTable parserCodeTable ParserCode 100
    ]

instance Code TestCode where
  codeMeta c = case Map.lookup c codeTable of
    Just m -> m
    Nothing -> error "unregistered code"

parseToPretty :: FilePath -> IO LBS.ByteString
parseToPretty fp = do
  src <- T.readFile fp
  let f = newFile fp src
  withSystemTempFile "reporter-output" $ \path h -> do
    let r = fileReporter h
    try @SomeException (lex lexConfig (contramap LexerCode r) f) >>= \case
      Left err -> pure $ TLE.encodeUtf8 $ "lex error:\n" <> TL.show err
      Right tokens -> do
        try @SomeException (parse parseConfig (contramap ParserCode r) f tokens) >>= \case
          Left err -> pure $ TLE.encodeUtf8 $ "parse error:\n" <> TL.show err
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
                  , vsep $ dprettyWithConfigs parseConfig lexConfig <$> ns
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
      [ goldenVsString (takeBaseName ntnFile) outputFile (parseToPretty ntnFile)
      | ntnFile <- ntnFiles
      , let outputFile = replaceExtension ntnFile ".output"
      ]
