module Main (main) where

import Prelude hiding (lex)
import Data.ByteString.Lazy qualified as LBS
import Data.Vector qualified as V
import Data.Text.IO.Utf8 qualified as T
import Data.Text.Lazy.Encoding qualified as TLE
import Geolog.CoreOperations (quote)
import Geolog.Lexer (lex)
import Geolog.Parser (parse)
import Geolog.Diagnostician
import Geolog.Elaborator (elabTop)
import Geolog.Pretty
import Geolog.Common
import Geolog.Core
import Prettyprinter
import Prettyprinter.Render.Text
import Test.Tasty (defaultMain, TestTree, testGroup)
import Test.Tasty.Golden (goldenVsString, findByExtension)
import System.FilePath (takeBaseName, replaceExtension)
import System.IO
import System.IO.Temp (withSystemTempFile)

main :: IO ()
main = defaultMain =<< goldenTests

render :: Doc ann -> LBS.ByteString
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
    pure $ render $ vsep [
      "-- tokens",
      vsep $ pretty <$> V.toList ts,
      "",
      "-- notation",
      vsep $ pretty <$> ns,
      "",
      "-- messages",
      pretty $ msgs]

prettyDecls :: GlobalEnv -> Doc ann
prettyDecls ge =
  let
    ?names = BwdNil
    ?ctxLen = 0 in vsep $ go (globalEntries ge) where
  go [] = []
  go ((x, PEntry t _ a):ds) =
    [ "potential entry named" <+> pretty x
    , "type: " <+> prtTop (quote a)
    , "value: " <+> prtTop t ] ++ go ds
  go ((x, KEntry t _ a):ds) =
    [ "kinetic entry named" <+> pretty x
    , "type:" <+> prtTop (quote a)
    , "value:" <+> prtTop t ] ++ go ds

elaborate :: FilePath -> IO LBS.ByteString
elaborate fp = do
  src <- T.readFile fp
  let f = newFile fp src
  withSystemTempFile "reporter-output" $ \path h -> do
    let r = Reporter h False
    ts <- lex r f
    ns <- parse r f ts
    ge <- elabTop r f ns
    hFlush h
    hClose h
    msgs <- T.readFile path
    pure $ render $ vsep [
      "-- tokens",
      vsep $ pretty <$> V.toList ts,
      "",
      "-- notation",
      vsep $ pretty <$> ns,
      "",
      "-- elaborated",
      prettyDecls ge,
      "",
      "-- messages",
      pretty $ msgs]

parserTests :: IO TestTree
parserTests = do
  ntnFiles <- findByExtension [".ntn"] "."
  return $ testGroup "Parser golden tests"
    [ goldenVsString (takeBaseName ntnFile) outputFile (parseToPretty ntnFile)
    | ntnFile <- ntnFiles
    , let outputFile = replaceExtension ntnFile ".output"
    ]

elaboratorTests :: IO TestTree
elaboratorTests = do
  glogFiles <- findByExtension [".glog"] "."
  return $ testGroup "Elaborator golden tests"
    [ goldenVsString (takeBaseName glogFile) outputFile (elaborate glogFile)
    | glogFile <- glogFiles
    , let outputFile = replaceExtension glogFile ".output"
    ]

goldenTests :: IO TestTree
goldenTests = do
  ts <- mapM id [parserTests, elaboratorTests]
  return $ testGroup "All tests" ts
