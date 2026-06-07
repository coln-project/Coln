module Main (main) where

import Data.ByteString.Lazy qualified as LBS
import Data.Functor.Contravariant (contramap)
import Data.Text.IO.Utf8 qualified as T
import Data.Text.Lazy.Encoding qualified as TLE
import Diagnostician
import FNotation
import Coln.Common
import Coln.Core.Params
import Coln.Core.Syntax
import Coln.Core.Print
import Coln.Diagnostics
import Coln.Report
import Coln.Frontend.Driver
import Coln.Frontend.Notation
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

prettyDecls :: Globals -> DDoc
prettyDecls ge = vsep $ go (toList ge)
 where
  go [] = []
  go ((x, GlobalEntry t _ a) : ds) =
    [ "global entry named" <+> dpretty x
    , "type: " <+> prtIn (CtxShape 0 BwdNil) a
    , "value: " <+> dprettyWithNames mempty t
    ]
      ++ go ds

elaborate :: FilePath -> IO LBS.ByteString
elaborate fp = do
  src <- T.readFile fp
  let f = newFile fp src
  withSystemTempFile "reporter-output" $ \path h -> do
    let r = fileReporter h
    ts <- lex lexConfig (contramap LexerCode r) f
    ns <- parse parseConfig (contramap ParserCode r) f ts
    ge <- top (DiagnosticEnv r f) ns
    hFlush h
    hClose h
    msgs <- T.readFile path
    pure $
      render $
        vsep
          [ "-- elaborated"
          , prettyDecls ge
          , ""
          , "-- messages"
          , pretty $ msgs
          ]

elaboratorTests :: IO TestTree
elaboratorTests = do
  glogFiles <- findByExtension [".glog"] "."
  return $
    testGroup
      "Elaborator golden tests"
      [ goldenVsString (takeBaseName glogFile) outputFile (elaborate glogFile)
      | glogFile <- glogFiles
      , let outputFile = replaceExtension glogFile ".output"
      ]

goldenTests :: IO TestTree
goldenTests = do
  ts <- mapM id [elaboratorTests]
  return $ testGroup "All tests" ts
