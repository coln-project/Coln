module Forester where

import Development.Shake
import Development.Shake.FilePath
import Servant hiding (ServerSentEvents)
import Servant.Types.SourceT
import Data.String (fromString)
import Servant.API.EventStream
import Network.Wai.Handler.Warp
import Data.Function ((&))
import System.FSNotify qualified as FSN
import Control.Monad (forever)
import Control.Concurrent (threadDelay)
import Control.Concurrent.Async
import Control.Concurrent.Chan

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
  "http://forester-builds.s3-website.us-east-2.amazonaws.com/forester-" <>
    spec <>
    ".tar.gz"

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
  cmd_ "ln" "-s" foresterExe "manual/forester"

data Refresh = Refresh

instance ToServerEvent Refresh where
  toServerEvent Refresh = dataEvent $ fromString "refresh"

type API =
  "refresh" :> ServerSentEvents (SourceIO Refresh)
  :<|> Raw

type RefreshChan = Chan Refresh

refreshServer :: Maybe RefreshChan -> Handler (SourceIO Refresh)
refreshServer Nothing = pure $ source []
refreshServer (Just refreshChan) = do
  clientChan <- liftIO (dupChan refreshChan)
  let refreshSteps = Effect $ do
        r <- readChan clientChan
        pure (Yield r refreshSteps)
  pure $ fromStepT refreshSteps

server :: Maybe RefreshChan -> Server API
server refreshChan =
  refreshServer refreshChan
  :<|> serveDirectoryFileServer "manual/output/"

foresterApp :: Maybe RefreshChan -> Application
foresterApp refreshChan = serve (Proxy @API) (server refreshChan)

buildForester :: IO ()
buildForester = do
  cmd_ (Cwd "manual") "./forester" "build"

continuouslyBuildForester :: RefreshChan -> IO ()
continuouslyBuildForester refreshChan = do
  FSN.withManager $ \mgr -> do
    let action = \case
          FSN.CloseWrite _ _ _ -> pure ()
          _ -> do
            buildForester
            writeChan refreshChan Refresh
    _ <- FSN.watchTree mgr "manual/trees" (const True) action
    _ <- FSN.watchTree mgr "manual/theme" (const True) action
    forever $ threadDelay 100000

serveForester :: Maybe RefreshChan -> IO ()
serveForester refreshChan = do
  (port, sock) <- openFreePort
  let beforeMainLoop = do
        putStrLn $ "running on port " ++ show port
        cmd_ "firefox" ("http://localhost:" ++ show port)
  let settings = defaultSettings & setBeforeMainLoop beforeMainLoop
  runSettingsSocket settings sock (foresterApp refreshChan)
    
foresterActions :: Rules ()
foresterActions = do
  "manual/forester" %> \_ -> linkForester

  phony "manual" $ do
    need ["manual/forester"]
    liftIO $ do
      buildForester
      cmd_ "mkdir -p" "_build/site/manual"
      cmd_ "cp -r" "manual/output" "_build/site/manual"

  phony "serve-manual" $ do
    liftIO $ do
      buildForester
      serveForester Nothing

  phony "dev-manual" $ liftIO $ do
    buildForester
    refreshChan <- newChan
    concurrently_
      (continuouslyBuildForester refreshChan)
      (serveForester $ Just refreshChan)
