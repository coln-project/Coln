module Geolog.LSP.DocChange (docChangeHandler, docOpenHandler) where

import Control.Lens ((^.))
import Control.Monad.Catch (MonadCatch)
import Control.Monad.Trans
import Data.IORef (modifyIORef')
import Data.Map qualified as M
import Diagnostician (newFile)
import Geolog.LSP.Buffer
import Geolog.LSP.Diagnostics (publishDiagnostics)
import Geolog.LSP.Types
import Geolog.LSP.Utils (currentBufferText, currentBufferUri, currentBufferUriUnNormalized)
import Language.LSP.Protocol.Lens (HasParams, HasTextDocument, HasUri)
import Language.LSP.Protocol.Message (SMethod (..))
import Language.LSP.Protocol.Types (Uri)
import Language.LSP.Server (Handlers, MonadLsp, getConfig, notificationHandler)
import Prelude hiding (lex)

docOpenHandler :: Handlers DLogLspM
docOpenHandler = notificationHandler SMethod_TextDocumentDidOpen updateState

docChangeHandler :: Handlers DLogLspM
docChangeHandler = notificationHandler SMethod_TextDocumentDidChange updateState

updateState ::
  ( MonadIO m,
    MonadCatch m,
    MonadLsp LSPState m,
    HasParams s a1,
    HasTextDocument a1 a2,
    HasUri a2 Uri
  ) =>
  s ->
  m ()
updateState req = do
  (bufferText, bufferUriNormalised, bufferUri) <- (,currentBufferUri req,req ^. currentBufferUriUnNormalized) <$> currentBufferText req

  let bufferFile = newFile (show bufferUri) bufferText
      bufInfo =
        LSPBufferInfo
          { uri = bufferUri,
            uriNormalised = bufferUriNormalised,
            file = bufferFile
          }

  result <- analyzeBuffer bufInfo
  updateParseState result bufInfo
  publishDiagnostics result.diagnostics bufInfo

updateParseState :: (MonadIO m, MonadLsp LSPState m) => AnalyzedBuffer -> LSPBufferInfo -> m ()
updateParseState a bufInfo = do
  ref <- (.parseState) <$> getConfig
  liftIO $ modifyIORef' ref (M.insert bufInfo.uriNormalised a)
