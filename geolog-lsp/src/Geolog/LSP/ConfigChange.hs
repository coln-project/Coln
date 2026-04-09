module Geolog.LSP.ConfigChange where

import Geolog.LSP.Types (GLogLspM)
import Language.LSP.Protocol.Message
import Language.LSP.Server

workspaceChangeConfigurationHandler :: Handlers GLogLspM
workspaceChangeConfigurationHandler = notificationHandler SMethod_WorkspaceDidChangeConfiguration \_ -> pure ()
