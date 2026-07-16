-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Main (main) where

import Coln.Backend.Lower
import Coln.Backend.TypeScript.Generate qualified as TypeScript
import Coln.Common
import Coln.Core
import Coln.Diagnostics
import Coln.Frontend.Notation
import Coln.Frontend.Parser
import Coln.Report
import Control.Exception (onException)
import Data.ByteString.Lazy qualified as LBS
import Data.Functor.Contravariant (contramap)
import Data.Map.Ordered qualified as OMap
import Data.Text.IO.Utf8 qualified as T
import Data.Text.Lazy.Encoding qualified as TLE
import FNotation
import Prettyprinter
import Prettyprinter.Render.Text
import System.Directory (createDirectoryIfMissing, doesFileExist, listDirectory, removeDirectoryRecursive, removePathForcibly)
import System.FilePath (replaceExtension, takeBaseName, takeExtension, (</>))
import System.IO
import System.IO.Temp (withSystemTempFile)
import Test.Tasty (DependencyType (AllSucceed), TestTree, defaultMain, dependentTestGroup, testGroup, withResource)
import Test.Tasty.Golden (findByExtension, goldenVsFile, goldenVsString)
import Test.Tasty.HUnit (testCase, (@?=))
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

loadGlobals :: FilePath -> IO (Globals, Text)
loadGlobals fp = do
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
    pure (ge, msgs)

elaborate :: FilePath -> IO LBS.ByteString
elaborate fp = do
  (ge, msgs) <- loadGlobals fp
  pure $
    render $
      vsep
        [ "-- elaborated"
        , prettyDecls ge
        , ""
        , "-- messages"
        , pretty $ msgs
        ]

generateTypeScript :: FilePath -> FilePath -> IO ()
generateTypeScript fp outdir = do
  createDirectoryIfMissing True outdir
  (ge, _) <- loadGlobals fp
  TypeScript.generate ge outdir

typescriptFiles :: FilePath -> IO [FilePath]
typescriptFiles directory =
  filter (\path -> takeExtension path `elem` [".json", ".ts"])
    <$> listDirectory directory

elaboratorTests :: IO TestTree
elaboratorTests = do
  colnFiles <- findByExtension [".coln"] "test/golden"
  return $
    testGroup
      "Elaborator golden tests"
      [ goldenVsString (takeBaseName colnFile) outputFile (elaborate colnFile)
      | colnFile <- colnFiles
      , let outputFile = replaceExtension colnFile ".output"
      ]

typescriptTests :: IO TestTree
typescriptTests = do
  colnFiles <- findByExtension [".coln"] "test/golden/basic-ir"
  tests <- mapM typescriptTest colnFiles
  return $ testGroup "TypeScript FFI golden tests" tests

typescriptTest :: FilePath -> IO TestTree
typescriptTest colnFile = do
  let name = takeBaseName colnFile
      goldenDir = replaceExtension colnFile ".ts.output"
      outputDir = replaceExtension colnFile ".test"

  createDirectoryIfMissing True goldenDir
  expectedFiles <- typescriptFiles goldenDir

  pure $
    withResource
      ( generateTypeScript colnFile outputDir
          `onException` removePathForcibly outputDir
      )
      (\_ -> removeDirectoryRecursive outputDir)
      (\_ ->
          dependentTestGroup
            name
            AllSucceed
            [ missingGoldenFilesTest goldenDir outputDir expectedFiles
            , testGroup
                "goldens"
                [ goldenVsFile
                    path
                    (goldenDir </> path)
                    (outputDir </> path)
                    (pure ()) -- File already generated
                | path <- expectedFiles
                ]
            ]
      )

missingGoldenFilesTest :: FilePath -> FilePath -> [FilePath] -> TestTree
missingGoldenFilesTest goldenDir outputDir expectedFiles =
  testCase "generated files have goldens" $ do
    actualFiles <- typescriptFiles outputDir
    mapM_ (touchMissingGolden goldenDir) actualFiles
    actualFiles @?= expectedFiles

touchMissingGolden :: FilePath -> FilePath -> IO ()
touchMissingGolden goldenDir path = do
  let goldenFile = goldenDir </> path
  exists <- doesFileExist goldenFile
  if exists then pure () else LBS.writeFile goldenFile mempty

goldenTests :: IO TestTree
goldenTests = do
  ts <- mapM id [elaboratorTests, typescriptTests]
  return $ testGroup "All tests" ts
