module Geolog.LSP.Diagnostics where

import Control.Monad.IO.Class (MonadIO)
import Control.Monad.Trans (lift)
import Control.Monad.Trans.Reader (ask)
import Data.Maybe (maybeToList)
import Data.Text qualified as T
import Diagnostician qualified as D
import Geolog.LSP.Types (LSPBufferInfo (..), LSPBufferT, LSPState)
import Language.LSP.Protocol.Message
import Language.LSP.Protocol.Types
import Language.LSP.Server

publishDiagnostics :: (MonadIO m, MonadLsp LSPState m, D.Code a) => [D.Diagnostic a] -> LSPBufferT m ()
publishDiagnostics ds = do
  bufInfo <- ask
  lift $
    sendNotification
      SMethod_TextDocumentPublishDiagnostics
      PublishDiagnosticsParams
        { _uri = bufInfo.uri
        , _version = Nothing
        , _diagnostics = concatMap (locToDiag bufInfo.file) ds
        }

locToDiag :: (D.Code a) => D.File -> D.Diagnostic a -> [Diagnostic]
locToDiag f d =
  fmap
    ( \loc ->
        Diagnostic
          { _range = loc
          , _severity = Just $ lspSeverityLevel d
          , _code = Just . InL . fromIntegral . (.number) . D.codeMeta $ d.code
          , _codeDescription = Nothing
          , _source = Nothing
          , _message = T.pack . show $ d.summary
          , _tags = Nothing
          , _relatedInformation = Nothing
          , _data_ = Nothing
          }
    )
    (diagnosticLocations d)
 where
  diagnosticLocations (D.Diagnostic _ _ ns) = case ns of
    -- Handles the case where a diagnostic doesn't have a location
    [] -> [Range (Position 1 1) (Position 1 1)]
    -- Diagnostics can have multiple locations, in this case we duplicate the diagnostic message
    _ -> do
      n <- ns
      sourceLoc <- maybeToList n.noteSourceLoc
      let D.Span start end = sourceLoc.span
      pure $ Range (posToPosition start) (posToPosition end)
  posToPosition p = Position (fromIntegral line) (fromIntegral col)
   where
    (line, col) = D.srcOf f p
  lspSeverityLevel = metaToLspSeverity . (.severity) . D.codeMeta . (.code)
  metaToLspSeverity = \case
    D.SDebug -> DiagnosticSeverity_Information
    D.SInfo -> DiagnosticSeverity_Information
    D.SWarning -> DiagnosticSeverity_Warning
    D.SError -> DiagnosticSeverity_Error
