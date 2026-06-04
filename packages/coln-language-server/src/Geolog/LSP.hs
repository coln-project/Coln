module Coln.LSP (serverDefinition) where

import Control.Monad.IO.Class
import Coln.LSP.DocChange (docChangeHandler, docOpenHandler)
import Coln.LSP.Highlight (tokenHandler)
import Coln.LSP.TrivialHandlers (cancelRequestHandler, didCloseHandler, initHandler, workspaceChangeConfigurationHandler)
import Coln.LSP.Types (GLogLspM, LSPState)
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

serverDefinition :: LSPState -> ServerDefinition LSPState
serverDefinition context =
  ServerDefinition
    { parseConfig = \c _ -> Right c
    , onConfigChange = const $ pure ()
    , defaultConfig = context
    , configSection = "geolog"
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
