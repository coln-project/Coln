module Main (main) where

import Data.IORef
import Geolog.LSP
import Geolog.LSP.Types
import Language.LSP.Server

main :: IO Int
main = do
  ref <- newIORef mempty
  runServer $
    serverDefinition
      LSPState
        { parseState = ref
        }
