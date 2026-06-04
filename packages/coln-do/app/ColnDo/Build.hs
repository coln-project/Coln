module ColnDo.Build where

import ColnDo.Common
import System.Directory (createDirectoryIfMissing)
import System.Info qualified

buildRules :: Rules ()
buildRules = do
  phony "build-vscode-extension" $ do
    let lsDir = "packages/coln-language-server"
    let clientDir = lsDir </> "client"
    let serverDir = lsDir </> "client/server" </> (System.Info.arch <> "-" <> System.Info.os)
    liftIO $ do
      removeFiles (lsDir </> "client") ["out", "server", "*.vsix"]
      createDirectoryIfMissing True serverDir
    cmd_ "cabal build coln-language-server"
    StdoutTrim binary <- cmd "cabal list-bin coln-language-server"
    copyFileChanged binary $ serverDir </> "coln-language-server"
    cmd_ (Cwd clientDir) "npm install"
    cmd_ (Cwd clientDir) "npm run compile"
    cmd_ (Cwd clientDir) "npm prune --production"
    cmd_ (Cwd clientDir) "npx --yes @vscode/vsce package --allow-missing-repository"

  phony "build-haskell" $ do
    cmd_ "cabal build all"

  phony "build-rust" $ do
    cmd_ "cargo build"

  phony "build" $ do
    need ["build-haskell", "build-rust", "build-vscode-extension"]
