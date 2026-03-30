module Geolog.LSP.Buffer (LSPBufferInfo (..), LSPBufferT, LSPBuffer, analyzeBuffer, AnalyzedBuffer (..)) where

import Control.Monad.Trans
import Control.Monad.Trans.Reader (ReaderT, ask)
import Data.Functor.Contravariant (contramap)
import Data.IORef (newIORef, readIORef)
import Data.Vector (Vector)
import Diagnostician qualified as D
import FNotation
import FNotation.Tokens (Token)
import Geolog.Core (GlobalEnv)
import Geolog.Elaborator (elabTop)
import Geolog.Notation (lexConfig, parseConfig)
import Language.LSP.Protocol.Types (NormalizedUri, Uri)
import Prelude hiding (lex)
import Control.Monad.Identity (Identity)
import Geolog.Diagnostics (GeologCode (..))


data LSPBufferInfo = LSPBufferInfo {
    uri :: Uri,
    uriNormalised :: NormalizedUri,
    file :: D.File
  }

type LSPBufferT m = ReaderT LSPBufferInfo m

type LSPBuffer = LSPBufferT Identity

data AnalyzedBuffer = AnalyzedBuffer {
    tokens :: Vector Token,
    notations :: [Ntn],
    elaborated :: GlobalEnv,
    diagnostics :: [D.Diagnostic GeologCode]
  }

analyzeBuffer :: (MonadIO m) => LSPBufferT m AnalyzedBuffer
analyzeBuffer = do
  bufInfo <- ask

  liftIO $ do
    diagRef <- newIORef ([] @(D.Diagnostic GeologCode))
    let r = D.pureReporter diagRef

    tokens <- lex lexConfig (contramap LexerCode r) bufInfo.file
    notations <- parse parseConfig (contramap ParserCode r) bufInfo.file tokens 
    elaborated <- elabTop (contramap ElaboratorCode r) bufInfo.file notations
    diagnostics <- readIORef diagRef

    pure $ AnalyzedBuffer {tokens, notations, elaborated, diagnostics}
