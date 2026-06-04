module Coln.LSP.Types where

import Data.IORef
import Data.Map (Map)
import Data.Vector
import Diagnostician qualified as D
import FNotation.Tokens (Token)
import FNotation.Trees (Ntn)
import Coln.Core (GlobalEnv)
import Coln.Diagnostics (ColnCode)
import Language.LSP.Protocol.Types qualified as J
import Language.LSP.Server (LspM)

data LSPBufferInfo = LSPBufferInfo
  { uri :: J.Uri
  , uriNormalised :: J.NormalizedUri
  , file :: D.File
  }

data AnalyzedBuffer = AnalyzedBuffer
  { raw :: D.File
  , tokens :: Maybe (Vector Token)
  , notations :: Maybe [Ntn]
  , elaborated :: Maybe GlobalEnv
  , diagnostics :: [D.Diagnostic ColnCode]
  }

type GLogLspM = LspM LSPState

type UriBundle a = Map J.NormalizedUri a

newtype LSPState = LSPState
  { parseState :: IORef (UriBundle AnalyzedBuffer)
  }
