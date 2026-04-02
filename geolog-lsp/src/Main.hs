import Data.IORef (newIORef)
import Geolog.LSP (serverDefinition)
import Geolog.LSP.Types (LSPState (..))
import Language.LSP.Server (runServer)

main :: IO Int
main = do
  ref <- newIORef mempty
  runServer $
    serverDefinition
      ( LSPState
          { parseState = ref
          }
      )
