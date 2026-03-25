module Geolog.LSP.Types  where

import Language.LSP.Server (LspM)
import Data.Text (Text)
import Data.Map (Map)
import Language.LSP.Protocol.Types qualified as J
import FNotation.Trees ( Ntn )
import Data.IORef

type DLogLspM = LspM LSPState

type UriBundle a = Map J.NormalizedUri a

data FileParseState = FileParseState {
  fileText :: Text,
  fileNtn :: [Ntn]
}

newtype LSPState = LSPState
    { parseState :: IORef (UriBundle FileParseState)
    }
