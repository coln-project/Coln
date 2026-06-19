module Coln.LSP (startServer) where

import Coln.LSP.DocChange (docChangeHandler, docOpenHandler)
import Coln.LSP.Highlight (tokenHandler)
import Coln.LSP.TrivialHandlers (cancelRequestHandler, didCloseHandler, initHandler, workspaceChangeConfigurationHandler)
import Coln.LSP.Types (GLogLspM, LSPState (..))
import Control.Monad.IO.Class
import Data.Functor (void)
import Data.IORef
import Language.LSP.Protocol.Types (TextDocumentSyncKind (..), TextDocumentSyncOptions (..))
import Language.LSP.Server

handlers :: Handlers GLogLspM
handlers =
  mconcat
    [ initHandler
    , workspaceChangeConfigurationHandler
    , cancelRequestHandler
    , didCloseHandler
    , docChangeHandler
    , docOpenHandler
    , tokenHandler
    ]

startServer :: IO ()
startServer =
  do
    ref <- newIORef mempty
    void
      . runServer
      $ serverDefinition
        LSPState
          { parseState = ref
          }

serverDefinition :: LSPState -> ServerDefinition LSPState
serverDefinition context =
  ServerDefinition
    { parseConfig = \c _ -> Right c
    , onConfigChange = const $ pure ()
    , defaultConfig = context
    , configSection = "coln"
    , doInitialize = \env _req -> pure $ Right env
    , staticHandlers = const handlers
    , interpretHandler = \env -> Iso (runLspT env) liftIO
    , options =
        defaultOptions
          { optTextDocumentSync =
              Just $
                TextDocumentSyncOptions
                  { _openClose = Just True
                  , _change = Just TextDocumentSyncKind_Full
                  , _willSave = Nothing
                  , _willSaveWaitUntil = Nothing
                  , _save = Nothing
                  }
          }
    }
