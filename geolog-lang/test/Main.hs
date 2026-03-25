module Main (main) where

import Prelude hiding (lex)
import Data.ByteString.Lazy qualified as LBS
import Data.Vector qualified as V
import Data.Text.IO.Utf8 qualified as T
import Data.Text.Lazy.Encoding qualified as TLE
import FNotation
import Diagnostician
import Geolog.Notation
import Geolog.Diagnostics
import Geolog.Elaborator (elabTop)
import Geolog.CoreOperations (prtVal)
import Geolog.Pretty
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

render :: DDoc -> LBS.ByteString
render = TLE.encodeUtf8 . renderLazy . layoutPretty defaultLayoutOptions

prettyDecls :: GlobalEnv -> DDoc
prettyDecls ge = vsep $ go (globalEntries ge) where
  go [] = []
  go ((x, PEntry t _ a):ds) =
    [ "potential entry named" <+> dpretty x
    , "type: " <+> prtVal mempty a
    , "value: " <+> dprettyWithNames mempty t ] ++ go ds
  go ((x, KEntry t _ a):ds) =
    [ "kinetic entry named" <+> dpretty x
    , "type:" <+> prtVal mempty a
    , "value:" <+> dprettyWithNames mempty t ] ++ go ds

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
    pure $ render $ vsep [
      "-- tokens",
      vsep $ dpretty <$> V.toList ts,
      "",
      "-- notation",
      vsep $ dpretty <$> ns,
      "",
      "-- elaborated",
      prettyDecls ge,
      "",
      "-- messages",
      pretty $ msgs]

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
  ts <- mapM id [elaboratorTests]
  return $ testGroup "All tests" ts
