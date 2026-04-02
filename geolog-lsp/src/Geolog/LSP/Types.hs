module Geolog.LSP.Types  where

import Language.LSP.Server (LspM)
import Data.Map (Map)
import Language.LSP.Protocol.Types qualified as J
import FNotation.Trees ( Ntn )
import Data.IORef
import qualified Diagnostician as D
import Control.Monad.Trans.Reader (ReaderT)
import Data.Functor.Identity (Identity)
import Data.Vector
import FNotation.Tokens (Token)
import Geolog.Core (GlobalEnv)
import Geolog.Diagnostics (GeologCode)

data LSPBufferInfo = LSPBufferInfo
  { uri :: J.Uri,
    uriNormalised :: J.NormalizedUri,
    file :: D.File
  }

type LSPBufferT m = ReaderT LSPBufferInfo m

type LSPBuffer = LSPBufferT Identity

data AnalyzedBuffer = AnalyzedBuffer
  { raw :: D.File,
    tokens :: Maybe (Vector Token),
    notations :: Maybe [Ntn],
    elaborated :: Maybe GlobalEnv,
    diagnostics :: [D.Diagnostic GeologCode]
  }

type DLogLspM = LspM LSPState

type UriBundle a = Map J.NormalizedUri a

newtype LSPState = LSPState
    { parseState :: IORef (UriBundle AnalyzedBuffer)
    }
