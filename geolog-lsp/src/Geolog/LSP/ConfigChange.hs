module Geolog.LSP.ConfigChange where

import Geolog.LSP.Types (DLogLspM)
import Language.LSP.Protocol.Message
import Language.LSP.Server

workspaceChangeConfigurationHandler :: Handlers DLogLspM
workspaceChangeConfigurationHandler = notificationHandler SMethod_WorkspaceDidChangeConfiguration \_ -> pure ()
