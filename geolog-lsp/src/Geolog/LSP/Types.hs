module Geolog.LSP.Types where

import Data.IORef
import Data.Map (Map)
import Data.Text (Text)
import FNotation.Trees (Ntn)
import Language.LSP.Protocol.Types qualified as J
import Language.LSP.Server (LspM)

type DLogLspM = LspM LSPState

type UriBundle a = Map J.NormalizedUri a

data FileParseState = FileParseState
  { fileText :: Text
  , fileNtn :: [Ntn]
  }

newtype LSPState = LSPState
  { parseState :: IORef (UriBundle FileParseState)
  }
