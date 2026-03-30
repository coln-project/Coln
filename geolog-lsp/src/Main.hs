import Geolog.LSP (serverDefinition)
import Geolog.LSP.Types (LSPState (..))
import Language.LSP.Server (runServer)
import Data.IORef (newIORef)
import GHC.Conc (setUncaughtExceptionHandler)
import Control.Exception

main :: IO Int
main = do
  -- setUncaughtExceptionHandler (\e -> writeFile "/home/patrick/Documents/geolog/lsp.log" $ show e )
  ref <- newIORef mempty
  runServer $
    serverDefinition
      ( LSPState
          { parseState = ref
          }
      )
