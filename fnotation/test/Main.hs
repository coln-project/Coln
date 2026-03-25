module Main (main) where

import Data.ByteString.Lazy qualified as LBS
import Data.Map (Map)
import Data.Map qualified as Map
import Data.Text.IO.Utf8 qualified as T
import Data.Text.Lazy.Encoding qualified as TLE
import Data.Vector qualified as V
import Diagnostician
import FNotation
import FNotation.Tokens qualified as K
import Prettyprinter
import Prettyprinter.Render.Text
import System.FilePath (replaceExtension, takeBaseName)
import System.IO
import System.IO.Temp (withSystemTempFile)
import Test.Tasty (TestTree, defaultMain, testGroup)
import Test.Tasty.Golden (findByExtension, goldenVsString)
import Prelude hiding (lex)

main :: IO ()
main = defaultMain =<< goldenTests

render :: DDoc -> LBS.ByteString
render = TLE.encodeUtf8 . renderLazy . layoutPretty defaultLayoutOptions

lexConfig :: ConfTable Kind
lexConfig =
  confTableFromList
    [ ("sig", K.Block)
    , ("struct", K.Block)
    , ("sum", K.Block)
    , ("match", K.Block)
    , ("theory", K.Decl)
    , ("def", K.Decl)
    , ("type", K.Decl)
    , ("let", K.Decl)
    , ("open", K.Decl)
    , ("import", K.Decl)
    , ("end", K.End)
    , ("Type", K.AKeyword)
    , ("Int", K.AKeyword)
    , ("String", K.AKeyword)
    , (":=", K.SKeyword)
    , ("=", K.SKeyword)
    , (":", K.SKeyword)
    , ("->", K.SKeyword)
    , ("=>", K.SKeyword)
    ]

parseConfig :: ConfTable Prec
parseConfig =
  confTableFromList
    [ (":=", Prec 10 AssocNon)
    , (":", Prec 20 AssocNon)
    , ("->", Prec 30 AssocR)
    , ("=>", Prec 30 AssocR)
    , ("=", Prec 40 AssocNon)
    , ("+", Prec 50 AssocL)
    , ("-", Prec 50 AssocL)
    , ("*", Prec 60 AssocL)
    , ("/", Prec 60 AssocL)
    ]

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
    let r = Reporter h False
    tokens <- lex lexConfig (ReporterFor LexerCode r) f
    ns <- parse parseConfig (ReporterFor ParserCode r) f tokens
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
          , "-- messages"
          , pretty $ msgs
          ]

goldenTests :: IO TestTree
goldenTests = do
  ntnFiles <- findByExtension [".ntn"] "."
  return $
    testGroup
      "FNotation golden tests"
      [ goldenVsString (takeBaseName ntnFile) outputFile (parseToPretty ntnFile)
      | ntnFile <- ntnFiles
      , let outputFile = replaceExtension ntnFile ".output"
      ]
