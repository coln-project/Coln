-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module ColnDo.Manual where

import ColnDo.Common

foresterVersion :: String
foresterVersion = "5.0-6e68237"

getSpec :: Action String
getSpec = do
  StdoutTrim osName <- cmd "uname"
  let os = case osName of
        "Darwin" -> "macos"
        "Linux" -> "linux"
        _ -> error $ "unsupported OS " <> osName
  StdoutTrim arch <- cmd "uname -m"
  pure $ foresterVersion <> "-" <> os <> "-" <> arch

foresterUrl :: String -> String
foresterUrl spec =
  "http://forester-builds.s3-website.us-east-2.amazonaws.com/forester-"
    <> spec
    <> ".tar.gz"

getForesterDir :: Action String
getForesterDir = do
  getEnv "HOME" >>= \case
    Just home ->
      pure $ home </> ".forester" </> foresterVersion
    Nothing -> error "HOME variable unset"

downloadForester :: Action ()
downloadForester = do
  withTempDir $ \tmp -> do
    spec <- getSpec
    let tarFile = tmp </> "forester.tar.gz"
    cmd_ "curl" "-o" tarFile (foresterUrl spec)
    foresterDir <- getForesterDir
    cmd_ (Cwd foresterDir) "tar" "-xf" tarFile "built"

linkForester :: Action ()
linkForester = do
  foresterDir <- getForesterDir
  let foresterExe = foresterDir </> "bin" </> "forester"
  doesFileExist foresterExe >>= \case
    True -> pure ()
    False -> downloadForester
  cmd_ "ln" "-s" foresterExe "bin/forester"

manualRules :: Rules ()
manualRules = do
  "bin/forester" %> \_ -> linkForester

  phony "manual" $ do
    need ["bin/forester"]
    liftIO $ do
      cmd_ (Cwd "manual") "../bin/forester" "build"
      cmd_ "mkdir -p" "_build/"
      cmd_ "rm -rf" "_build/manual"
      cmd_ "cp" "-r" "manual/output" "_build/manual"
