module Geolog.LSP.Buffer (LSPBufferInfo (..), LSPBufferT, LSPBuffer, analyzeBuffer) where

import Control.Monad.Identity (Identity)
import Control.Monad.Trans
import Control.Monad.Trans.Reader (ReaderT, ask)
import Data.Functor.Contravariant (contramap)
import Data.IORef (newIORef, readIORef)
import Data.Vector (Vector)
import Diagnostician qualified as D
import FNotation
import FNotation.Tokens (Token)
import Geolog.Core (GlobalEnv)
import Geolog.Diagnostics (GeologCode (..))
import Geolog.Elaborator (elabTop)
import Geolog.Notation (lexConfig, parseConfig)
import Language.LSP.Protocol.Types (NormalizedUri, Uri)
import Prelude hiding (lex)

data LSPBufferInfo = LSPBufferInfo
  { uri :: Uri
  , uriNormalised :: NormalizedUri
  , file :: D.File
  }

type LSPBufferT m = ReaderT LSPBufferInfo m

type LSPBuffer = LSPBufferT Identity

analyzeBuffer :: (MonadIO m) => LSPBufferT m (Vector Token, [Ntn], GlobalEnv, [D.Diagnostic GeologCode])
analyzeBuffer = do
  bufInfo <- ask

  liftIO $ do
    diagRef <- newIORef ([] @(D.Diagnostic GeologCode))
    let r = D.pureReporter diagRef

    ts <- lex lexConfig (contramap LexerCode r) bufInfo.file
    ns <- parse parseConfig (contramap ParserCode r) bufInfo.file ts
    ge <- elabTop (contramap ElaboratorCode r) bufInfo.file ns
    ds <- readIORef diagRef

    pure (ts, ns, ge, ds)
