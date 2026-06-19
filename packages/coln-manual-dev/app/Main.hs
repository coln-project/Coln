module Main where

import Control.Concurrent (threadDelay)
import Control.Concurrent.Async
import Control.Concurrent.Chan
import Control.Monad (forever)
import Control.Monad.IO.Class (liftIO)
import Data.Function ((&))
import Data.String (fromString)
import Network.Wai.Handler.Warp
import Servant hiding (ServerSentEvents)
import Servant.API.EventStream
import Servant.Types.SourceT
import System.FSNotify qualified as FSN
import System.Process

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
    :<|> serveDirectoryFileServer "output/"

foresterApp :: Maybe RefreshChan -> Application
foresterApp refreshChan = serve (Proxy @API) (server refreshChan)

buildForester :: IO ()
buildForester = callCommand "forester build"

continuouslyBuildForester :: RefreshChan -> IO ()
continuouslyBuildForester refreshChan = do
  FSN.withManager $ \mgr -> do
    let action = \case
          FSN.CloseWrite _ _ _ -> pure ()
          _ -> do
            buildForester
            writeChan refreshChan Refresh
    _ <- FSN.watchTree mgr "trees" (const True) action
    _ <- FSN.watchTree mgr "theme" (const True) action
    forever $ threadDelay 100000

serveForester :: Maybe RefreshChan -> IO ()
serveForester refreshChan = do
  (port, sock) <- openFreePort
  let beforeMainLoop = do
        putStrLn $ "running on port " ++ show port
        callCommand $ "firefox http://localhost:" ++ show port
  let settings = defaultSettings & setBeforeMainLoop beforeMainLoop
  runSettingsSocket settings sock (foresterApp refreshChan)

main :: IO ()
main = do
  buildForester
  refreshChan <- newChan
  concurrently_
    (continuouslyBuildForester refreshChan)
    (serveForester $ Just refreshChan)
