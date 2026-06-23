-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.LSP.DocChange (docChangeHandler, docOpenHandler) where

import Coln.LSP.Buffer
import Coln.LSP.Diagnostics (publishDiagnostics)
import Coln.LSP.Types
import Coln.LSP.Utils (currentBufferText, currentBufferUri, currentBufferUriUnNormalized)
import Control.Lens ((^.))
import Control.Monad.Catch (MonadCatch)
import Control.Monad.Trans
import Data.IORef (modifyIORef')
import Data.Map qualified as M
import Diagnostician (newFile)
import Language.LSP.Protocol.Lens (HasParams, HasTextDocument, HasUri)
import Language.LSP.Protocol.Message (SMethod (..))
import Language.LSP.Protocol.Types (Uri)
import Language.LSP.Server (Handlers, MonadLsp, getConfig, notificationHandler)
import Prelude hiding (lex)

docOpenHandler :: Handlers GLogLspM
docOpenHandler = notificationHandler SMethod_TextDocumentDidOpen updateState

docChangeHandler :: Handlers GLogLspM
docChangeHandler = notificationHandler SMethod_TextDocumentDidChange updateState

updateState ::
  ( MonadIO m
  , MonadCatch m
  , MonadLsp LSPState m
  , HasParams s a1
  , HasTextDocument a1 a2
  , HasUri a2 Uri
  ) =>
  s ->
  m ()
updateState req = do
  (bufferText, bufferUriNormalised, bufferUri) <- (,currentBufferUri req,req ^. currentBufferUriUnNormalized) <$> currentBufferText req

  let bufferFile = newFile (show bufferUri) bufferText
      bufInfo =
        LSPBufferInfo
          { uri = bufferUri
          , uriNormalised = bufferUriNormalised
          , file = bufferFile
          }

  result <- analyzeBuffer bufInfo
  updateParseState result bufInfo
  publishDiagnostics result.diagnostics bufInfo

updateParseState :: (MonadIO m, MonadLsp LSPState m) => AnalyzedBuffer -> LSPBufferInfo -> m ()
updateParseState a bufInfo = do
  ref <- (.parseState) <$> getConfig
  liftIO $ modifyIORef' ref (M.insert bufInfo.uriNormalised a)
