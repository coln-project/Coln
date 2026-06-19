-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

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

  let wasmBuildDir = "_build/wasm"
      wasmDistDir = wasmBuildDir </> "dist"
      wasmBin = wasmDistDir </> "coln.wasm"
      jsffi = wasmDistDir </> "ghc_wasm_jsffi.js"
  liftIO $ createDirectoryIfMissing True wasmDistDir
  wasmBin %> \_ -> do
    alwaysRerun
    cmd_ "wasm32-wasi-cabal build coln-wasm"
    StdoutTrim bin <- cmd "wasm32-wasi-cabal list-bin coln-wasm"
    copyFileChanged bin wasmBin
  jsffi %> \_ -> do
    need [wasmBin]
    StdoutTrim libdir <- cmd "wasm32-wasi-ghc --print-libdir"
    cmd_ (libdir </> "post-link.mjs") "--input" wasmBin "--output" jsffi
  phony "serve-example-frontend" $ do
    need [wasmBin, jsffi]
    copyFileChanged "packages/coln-wasm/example.html" (wasmBuildDir </> "index.html")
    runAfter $ cmd_ "simple-http-server --nocache --index --open" wasmBuildDir

  phony "build-rust" $ do
    cmd_ "cargo build"

  phony "build" $ do
    need ["build-haskell", "build-rust", "build-vscode-extension"]
