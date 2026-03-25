module Main (main) where

import Data.ByteString.Lazy qualified as LBS
import Data.Text.IO.Utf8 qualified as T
import Data.Text.Lazy.Encoding qualified as TLE
import Data.Vector qualified as V
import Geolog.Common
import Geolog.Core
import Geolog.CoreOperations (quote)
import Geolog.Diagnostician
import Geolog.Elaborator (elabTop)
import Geolog.Lexer (lex)
import Geolog.Parser (parse)
import Geolog.Pretty
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

parseToPretty :: FilePath -> IO LBS.ByteString
parseToPretty fp = do
  src <- T.readFile fp
  let f = newFile fp src
  withSystemTempFile "reporter-output" $ \path h -> do
    let r = Reporter h False
    ts <- lex r f
    ns <- parse r f ts
    hFlush h
    hClose h
    msgs <- T.readFile path
    pure $
      render $
        vsep
          [ "-- tokens",
            vsep $ pretty <$> V.toList ts,
            "",
            "-- notation",
            vsep $ pretty <$> ns,
            "",
            "-- messages",
            pretty $ msgs
          ]

prettyDecls :: GlobalEnv -> Doc ann
prettyDecls ge =
  let ?names = BwdNil
      ?ctxLen = 0
   in vsep $ go (globalEntries ge)
  where
    go [] = []
    go ((x, PEntry t _ a) : ds) =
      [ "potential entry named" <+> pretty x,
        "type: " <+> prtTop (quote a),
        "value: " <+> prtTop t
      ]
        ++ go ds
    go ((x, KEntry t _ a) : ds) =
      [ "kinetic entry named" <+> pretty x,
        "type:" <+> prtTop (quote a),
        "value:" <+> prtTop t
      ]
        ++ go ds

elaborate :: FilePath -> IO LBS.ByteString
elaborate fp = do
  src <- T.readFile fp
  let f = newFile fp src
  withSystemTempFile "reporter-output" $ \path h -> do
    let r = Reporter h False
    ts <- lex lexConfig (ReporterFor LexerCode r) f
    ns <- parse parseConfig (ReporterFor ParserCode r) f ts
    ge <- elabTop (ReporterFor ElaboratorCode r) f ns
    hFlush h
    hClose h
    msgs <- T.readFile path
    pure $
      render $
        vsep
          [ "-- tokens",
            vsep $ pretty <$> V.toList ts,
            "",
            "-- notation",
            vsep $ pretty <$> ns,
            "",
            "-- elaborated",
            prettyDecls ge,
            "",
            "-- messages",
            pretty $ msgs
          ]

parserTests :: IO TestTree
parserTests = do
  ntnFiles <- findByExtension [".ntn"] "."
  return $
    testGroup
      "Parser golden tests"
      [ goldenVsString (takeBaseName ntnFile) outputFile (parseToPretty ntnFile)
      | ntnFile <- ntnFiles,
        let outputFile = replaceExtension ntnFile ".output"
      ]

elaboratorTests :: IO TestTree
elaboratorTests = do
  glogFiles <- findByExtension [".glog"] "."
  return $
    testGroup
      "Elaborator golden tests"
      [ goldenVsString (takeBaseName glogFile) outputFile (elaborate glogFile)
      | glogFile <- glogFiles,
        let outputFile = replaceExtension glogFile ".output"
      ]

goldenTests :: IO TestTree
goldenTests = do
  ts <- mapM id [elaboratorTests]
  return $ testGroup "All tests" ts
