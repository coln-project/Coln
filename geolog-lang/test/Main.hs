module Main (main) where

import Prelude hiding (lex)
import Data.Vector qualified as V
import Data.ByteString qualified as BS
import Data.ByteString.Lazy qualified as LBS
import Data.Text.Lazy.Encoding qualified as TLE
import Data.Text.Encoding qualified as TE
import Geolog.Lexer (lex)
import Geolog.Parser (parse)
import Geolog.Diagnostics
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
  bs <- BS.readFile fp
  let f = newFile fp bs
  withSystemTempFile "reporter-output" $ \path h -> do
    let r = Reporter h False
    ts <- lex r f
    ns <- parse r f ts
    hFlush h
    hClose h
    msgs <- BS.readFile path
    pure $ render $ vsep [
      "-- tokens",
      vsep $ pretty <$> V.toList ts,
      "",
      "-- notation",
      vsep $ pretty <$> ns,
      "",
      "-- messages",
      pretty $ TE.decodeUtf8 msgs]

goldenTests :: IO TestTree
goldenTests = do
  ntnFiles <- findByExtension [".ntn"] "."
  return $ testGroup "Geolog golden tests"
    [ goldenVsString (takeBaseName ntnFile) outputFile (parseToPretty ntnFile)
    | ntnFile <- ntnFiles
    , let outputFile = replaceExtension ntnFile ".output"
    ]
