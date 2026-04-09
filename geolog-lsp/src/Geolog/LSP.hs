module Geolog.LSP (serverDefinition) where

import Control.Monad.IO.Class
import Geolog.LSP.ConfigChange (workspaceChangeConfigurationHandler)
import Geolog.LSP.DocChange (docChangeHandler, docOpenHandler)
import Geolog.LSP.Highlight (tokenHandler)
import Geolog.LSP.Types (GLogLspM, LSPState)
import Language.LSP.Protocol.Message (SMethod (..))
import Language.LSP.Protocol.Types (TextDocumentSyncKind (..), TextDocumentSyncOptions (..))
import Language.LSP.Server

handlers :: Handlers GLogLspM
handlers =
  mconcat
    [ initHandler,
      docChangeHandler,
      docOpenHandler,
      tokenHandler,
      workspaceChangeConfigurationHandler
    ]

initHandler :: Handlers GLogLspM
initHandler = notificationHandler SMethod_Initialized $ \_ -> pure ()

serverDefinition :: LSPState -> ServerDefinition LSPState
serverDefinition context =
  ServerDefinition
    { parseConfig = \c _ -> Right c,
      onConfigChange = const $ pure (),
      defaultConfig = context,
      configSection = "geolog",
      doInitialize = \env _req -> pure $ Right env,
      staticHandlers = const handlers,
      interpretHandler = \env -> Iso (runLspT env) liftIO,
      options =
        defaultOptions
          { optTextDocumentSync =
              Just $
                TextDocumentSyncOptions
                  { _openClose = Just True,
                    _change = Just TextDocumentSyncKind_Full,
                    _willSave = Nothing,
                    _willSaveWaitUntil = Nothing,
                    _save = Nothing
                  }
          }
    }
