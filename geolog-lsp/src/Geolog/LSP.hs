module Geolog.LSP (serverDefinition) where

import Control.Monad.IO.Class
import Geolog.LSP.DocChange (docChangeHandler, docOpenHandler)
import Geolog.LSP.Highlight (tokenHandler)
import Geolog.LSP.Types (DLogLspM, LSPState)
import Language.LSP.Protocol.Message (SMethod (..))
import Language.LSP.Protocol.Types (SaveOptions (..), TextDocumentSyncKind (..), TextDocumentSyncOptions (..), type (|?) (InR))
import Language.LSP.Server

handlers :: Handlers DLogLspM
handlers =
  mconcat
    [ initHandler,
      docChangeHandler,
      docOpenHandler,
      tokenHandler
    ]

initHandler :: Handlers DLogLspM
initHandler = notificationHandler SMethod_Initialized $ \_ -> pure ()

serverDefinition :: LSPState -> ServerDefinition LSPState
serverDefinition context =
  ServerDefinition
    { parseConfig = \c _ -> Right c,
      onConfigChange = const $ pure (),
      defaultConfig = context,
      configSection = "demo",
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
                    _willSave = Just False,
                    _willSaveWaitUntil = Just False,
                    _save = Just (InR (SaveOptions (Just False)))
                  }
          }
    }
