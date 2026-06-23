-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT
{-# LANGUAGE DataKinds #-}

module Coln.LSP.TrivialHandlers where

import Coln.LSP.Types (GLogLspM)
import Language.LSP.Protocol.Message
import Language.LSP.Server

initHandler :: Handlers GLogLspM
initHandler = empty SMethod_Initialized

workspaceChangeConfigurationHandler :: Handlers GLogLspM
workspaceChangeConfigurationHandler = empty SMethod_WorkspaceDidChangeConfiguration

cancelRequestHandler :: Handlers GLogLspM
cancelRequestHandler = empty SMethod_CancelRequest

didCloseHandler :: Handlers GLogLspM
didCloseHandler = empty SMethod_TextDocumentDidClose

empty :: (Applicative f) => SMethod (m :: Method ClientToServer Notification) -> Handlers f
empty hname = notificationHandler hname \_ -> pure ()
