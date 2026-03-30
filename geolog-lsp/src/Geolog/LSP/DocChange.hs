{-# LANGUAGE BlockArguments #-}

module Geolog.LSP.DocChange (docChangeHandler, docOpenHandler) where

import Control.Lens ((^.))
import Control.Monad.Catch (MonadCatch)
import Control.Monad.Trans
import Control.Monad.Trans.Reader (ask, runReaderT)
import Data.IORef (modifyIORef')
import Data.Map qualified as M
import Diagnostician (newFile)
import Diagnostician qualified as D
import FNotation
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
  ( MonadIO m
  , MonadCatch m
  , MonadLsp LSPState m
  , HasParams s a1
  , HasTextDocument a1 a2
  , HasUri a2 Uri
  ) =>
  s -> m ()
updateState req = do
  (bufferText, bufferUriNormalised, bufferUri) <- (,currentBufferUri req,req ^. currentBufferUriUnNormalized) <$> currentBufferText req

  let bufferFile = newFile (show bufferUri) bufferText
      bufInfo =
        LSPBufferInfo
          { uri = bufferUri
          , uriNormalised = bufferUriNormalised
          , file = bufferFile
          }

  flip runReaderT bufInfo $ do
    result <- analyzeBuffer
    mapM_ updateParseState result.notations
    publishDiagnostics result.diagnostics

updateParseState :: (MonadIO m, MonadLsp LSPState m) => [Ntn] -> LSPBufferT m ()
updateParseState ns = do
  bufInfo <- ask
  ref <- (.parseState) <$> lift getConfig
  liftIO $ modifyIORef' ref (M.insert bufInfo.uriNormalised (FileParseState bufInfo.file.contents ns))
