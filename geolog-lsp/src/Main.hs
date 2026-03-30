import Control.Exception
import Data.IORef (newIORef)
import GHC.Conc (setUncaughtExceptionHandler)
import Geolog.LSP (serverDefinition)
import Geolog.LSP.Types (LSPState (..))
import Language.LSP.Server (runServer)

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
