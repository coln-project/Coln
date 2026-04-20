module Main where

import Control.Exception.Extra
import Control.Monad (forM_)
import Data.ByteString qualified as BS
import Data.ByteString.Builder qualified as BSB
import Data.String (fromString)
import Development.Shake
import Development.Shake.FilePath
import Djot
import GHC.Stack (withFrozenCallStack)
import System.Directory (createDirectoryIfMissing)
import System.Environment (setEnv)
import System.Info qualified

ignoreTheseProjects :: [String]
ignoreTheseProjects = ["toy-datalog", "felix-db"]

getProjects :: Action [String]
getProjects = do
  cabalFiles <- getDirectoryFiles "" ["*/*.cabal"]
  let allProjects = takeDirectory <$> cabalFiles
  pure $ filter (not . flip elem ignoreTheseProjects) allProjects

projectPaths :: [String]
projectPaths = ["src", "test", "app"]

getHsFiles :: Action [String]
getHsFiles = do
  hsProjects <- getProjects
  let paths = [proj ++ "/" ++ path ++ "//*.hs" | proj <- hsProjects, path <- projectPaths]
  getDirectoryFiles "" paths

htmlPreamble :: BSB.Builder
htmlPreamble =
  fromString
    ( "<!doctype html>"
        <> "<html><head><meta charset=\"UTF-8\"/></head><body>"
    )

htmlPostamble :: BSB.Builder
htmlPostamble = fromString "</body></html>"

buildDjot :: FilePath -> FilePath -> IO ()
buildDjot srcPath outPath = do
  src <- BS.readFile srcPath
  case parseDoc (ParseOptions NoSourcePos) src of
    Left msg -> print ("Error while parsing " <> srcPath <> ":\n" <> msg)
    Right d -> do
      let b = renderHtml (RenderOptions False) d
      BSB.writeFile outPath (htmlPreamble <> b <> htmlPostamble)

shakeError :: String -> Action ()
shakeError msg = withFrozenCallStack $ liftIO $ errorIO msg

actions :: Rules ()
actions = do
  phony "format" $ do
    hsFiles <- getHsFiles
    putInfo ("Formatting:" <> mconcat (("\n - " ++) <$> hsFiles))
    cmd_ "ormolu --mode inplace" hsFiles
    projects <- getProjects
    forM_ projects $ \p ->
      cmd_ "cabal format" (p </> p ++ ".cabal")

  phony "check" $ do
    hsFiles <- getHsFiles
    putInfo ("Checking formatting")
    cmd_ "ormolu --mode check" hsFiles

  phony "build" $ do
    cmd_ "cabal build all"

  phony "test" $ do
    cmd_ "cabal test all --enable-tests"

  phony "clean" $ do
    removeFilesAfter "_build" ["//*"]

  phony "haddock" $ do
    -- for some reason, prettyprinter doesn't build documentation
    -- TODO: just ignore prettyprinter
    cmd_ "cabal haddock geolog-lang --disable-documentation --haddock-output-dir=_build/site/haddock"

  phony "tex" $ do
    texs <- getDirectoryFiles "docs" ["*.tex"]
    let pdfs = ["_build/site" </> tex -<.> "pdf" | tex <- texs]
    need pdfs

  phony "djot" $ do
    djs <- getDirectoryFiles "docs" ["*.dj"]
    let htmls = ["_build/site" </> dj -<.> "html" | dj <- djs]
    need htmls

  phony "docs" $ do
    need ["haddock", "tex", "djot"]

  "_build/site/*.html" %> \out -> do
    putInfo ("Building " <> out)
    let dj = "docs" </> (takeFileName out -<.> "dj")
    need [dj]
    liftIO $ buildDjot dj out

  "_build/site/*.pdf" %> \out -> do
    putInfo ("Building " <> out)
    support <- getDirectoryFiles "" ["docs/*.sty", "docs/*.bib"]
    let tex = "docs" </> (takeFileName out -<.> "tex")
    need [tex]
    cmd_ "tectonic -o _build/site" [tex]
    needed support

  -- Build VSCode extension
  phony "vsce" $ do
    let serverDir = "geolog-lsp/client/server" </> (System.Info.arch <> "-" <> System.Info.os)
    liftIO $ do
      removeFiles "geolog-lsp/client" ["out", "server", "*.vsix"]
      createDirectoryIfMissing True serverDir
    cmd_ "cabal build geolog-lsp"
    StdoutTrim binary <- cmd "cabal list-bin geolog-lsp"
    copyFileChanged binary $ serverDir </> "geolog-lsp"
    cmd_ (Cwd "geolog-lsp/client") "npm install"
    cmd_ (Cwd "geolog-lsp/client") "npm run compile"
    cmd_ (Cwd "geolog-lsp/client") "npm prune --production"
    cmd_ (Cwd "geolog-lsp/client") "npx --yes @vscode/vsce package --allow-missing-repository"

main :: IO ()
main = do
  setEnv "LANG" "en_US.UTF-8"
  shakeArgs shakeOptions {shakeColor = True} actions
