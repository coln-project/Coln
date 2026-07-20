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
import Control.Exception (evaluate, finally, onException)
import Data.ByteString.Lazy qualified as LBS
import Data.Functor.Contravariant (contramap)
import Data.List (partition)
import Data.Map.Ordered qualified as OMap
import Data.Text.IO.Utf8 qualified as T
import Data.Text.Lazy.Encoding qualified as TLE
import FNotation
import Prettyprinter
import Prettyprinter.Render.Text
import System.Directory (createDirectoryIfMissing, doesFileExist, listDirectory, removePathForcibly)
import System.FilePath (replaceExtension, takeBaseName, takeExtension, (</>))
import System.IO
import System.IO.Temp (withSystemTempFile)
import Test.Tasty (DependencyType (AllSucceed), TestTree, defaultMain, dependentTestGroup, testGroup, withResource)
import Test.Tasty.ExpectedFailure (expectFail)
import Test.Tasty.Golden (findByExtension, goldenVsFile, goldenVsString)
import Test.Tasty.HUnit (testCase, (@?=))
import Prelude hiding (lex, read)

knownFailingElaboratorTests :: [String]
knownFailingElaboratorTests = ["lookup-record-field"]

knownFailingTypeScriptTests :: [String]
knownFailingTypeScriptTests = ["equality", "equality-prop", "lookup-record-field", "rule-literals"]

main :: IO ()
main = defaultMain =<< goldenTests

goldenTests :: IO TestTree
goldenTests = do
  ts <- mapM id [elaboratorTests, typescriptTests]
  return $ testGroup "All tests" ts

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
  let (failingFiles, goldenFiles) = partition (isKnownFailure knownFailingElaboratorTests) colnFiles
  return $
    testGroup
      "Elaborator golden tests"
      [ testGroup "goldens" (elaboratorGoldenTest <$> goldenFiles)
      , testGroup "known failures" (elaboratorFailingTest <$> failingFiles)
      ]

elaboratorGoldenTest :: FilePath -> TestTree
elaboratorGoldenTest colnFile = goldenVsString name outputFile (elaborate colnFile)
 where
  name = takeBaseName colnFile
  outputFile = replaceExtension colnFile ".output"

elaboratorFailingTest :: FilePath -> TestTree
elaboratorFailingTest colnFile =
  expectFail $ testCase (takeBaseName colnFile) $ do
    output <- elaborate colnFile
    -- Force the elaboration to actually happen
    _ <- evaluate $ LBS.length output
    pure ()

typescriptTests :: IO TestTree
typescriptTests = do
  colnFiles <- findByExtension [".coln"] "test/golden/basic-ir"
  let (failingFiles, goldenFiles) = partition (isKnownFailure knownFailingTypeScriptTests) colnFiles
  goldenTestTrees <- mapM typescriptGoldenTest goldenFiles
  return $
    testGroup
      "TypeScript FFI golden tests"
      [ testGroup "goldens" goldenTestTrees
      , testGroup "known failures" (typescriptFailingTest <$> failingFiles)
      ]

typescriptGoldenTest :: FilePath -> IO TestTree
typescriptGoldenTest colnFile = do
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
      (\_ -> removePathForcibly outputDir)
      ( \_ ->
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

typescriptFailingTest :: FilePath -> TestTree
typescriptFailingTest colnFile =
  expectFail $
    testCase (takeBaseName colnFile) $
      generateTypeScript colnFile outputDir `finally` removePathForcibly outputDir
 where
  outputDir = replaceExtension colnFile ".test"

isKnownFailure :: [String] -> FilePath -> Bool
isKnownFailure knownFailures = (`elem` knownFailures) . takeBaseName

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
