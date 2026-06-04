module Coln.LSP.Diagnostics (publishDiagnostics) where

import Control.Monad.IO.Class (MonadIO)
import Data.Maybe (maybeToList)
import Data.Text qualified as T
import Diagnostician qualified as D
import Coln.LSP.Types (LSPBufferInfo (..), LSPState)
import Language.LSP.Protocol.Message
import Language.LSP.Protocol.Types
import Language.LSP.Server hiding (publishDiagnostics)

publishDiagnostics :: (MonadIO m, MonadLsp LSPState m, D.Code a) => [D.Diagnostic a] -> LSPBufferInfo -> m ()
publishDiagnostics ds bufInfo = do
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
    (diagnosticLocations f d)

diagnosticLocations :: D.File -> D.Diagnostic a -> [Range]
diagnosticLocations f (D.Diagnostic _ _ ns) = case ns of
  -- Handles the case where a diagnostic doesn't have a location
  [] -> [Range (Position 1 1) (Position 1 1)]
  -- Diagnostics can have multiple locations, in this case we duplicate the diagnostic message
  _ -> do
    n <- ns
    sourceLoc <- maybeToList n.noteSourceLoc
    let D.Span start end = sourceLoc.span
    pure $ Range (posToPosition f start) (posToPosition f end)

posToPosition :: D.File -> D.Pos -> Position
posToPosition f p = Position (fromIntegral line) (fromIntegral col)
 where
  (line, col) = D.srcOf f p

lspSeverityLevel :: (D.Code a) => D.Diagnostic a -> DiagnosticSeverity
lspSeverityLevel = metaToLspSeverity . (.severity) . D.codeMeta . (.code)

metaToLspSeverity :: D.Severity -> DiagnosticSeverity
metaToLspSeverity = \case
  D.SDebug -> DiagnosticSeverity_Information
  D.SInfo -> DiagnosticSeverity_Information
  D.SWarning -> DiagnosticSeverity_Warning
  D.SError -> DiagnosticSeverity_Error
