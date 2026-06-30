-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Main (main) where

import Coln.Backend.Lower
import Coln.Common
import Coln.Core.Globals
import Coln.Core.Params
import Coln.Core.Print
import Coln.Core.Realm
import Coln.Diagnostics
import Coln.Frontend.Driver
import Coln.Frontend.Notation
import Coln.Report
import Data.ByteString.Lazy qualified as LBS
import Data.Functor.Contravariant (contramap)
import Data.Map.Ordered qualified as OMap
import Data.Text.IO.Utf8 qualified as T
import Data.Text.Lazy.Encoding qualified as TLE
import Diagnostician
import FNotation
import Prettyprinter
import Prettyprinter.Render.Text
import System.FilePath (replaceExtension, takeBaseName)
import System.IO
import System.IO.Temp (withSystemTempFile)
import Test.Tasty (TestTree, defaultMain, testGroup)
import Test.Tasty.Golden (findByExtension, goldenVsString)
import Prelude hiding (lex, read)

main :: IO ()
main = defaultMain =<< goldenTests

render :: DDoc -> LBS.ByteString
render = TLE.encodeUtf8 . renderLazy . layoutPretty defaultLayoutOptions

prettyEntry :: (Name, GlobalEntry) -> DDoc
prettyEntry (x, (GlobalEntry t _ a)) =
  vsep
    [ "global entry named" <+> dpretty x
    , "type:" <+> prtIn (CtxShape 0 BwdNil) a
    , "value:" <+> dprettyWithNames mempty t
    ]

prettyRealm :: (Name, Realm) -> DDoc
prettyRealm (x, r) =
  vsep
    [ "realm named" <+> dpretty x
    , "generators:" <+> dpretty r
    , "lowered:" <+> dpretty (lowerRealm x r)
    ]

prettyDecls :: Globals -> DDoc
prettyDecls ge =
  vsep $
    (prettyEntry <$> OMap.assocs ge.entries)
      ++ (prettyRealm <$> OMap.assocs ge.realms)

elaborate :: FilePath -> IO LBS.ByteString
elaborate fp = do
  src <- T.readFile fp
  let f = newFile fp src
  withSystemTempFile "reporter-output" $ \path h -> do
    let r = fileReporter h
    ts <- lex lexConfig (contramap LexerCode r) f
    ns <- read readConfig (contramap ReaderCode r) f ts
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
  colnFiles <- findByExtension [".coln"] "."
  return $
    testGroup
      "Elaborator golden tests"
      [ goldenVsString (takeBaseName colnFile) outputFile (elaborate colnFile)
      | colnFile <- colnFiles
      , let outputFile = replaceExtension colnFile ".output"
      ]

goldenTests :: IO TestTree
goldenTests = do
  ts <- mapM id [elaboratorTests]
  return $ testGroup "All tests" ts
