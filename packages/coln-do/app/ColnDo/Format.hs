module ColnDo.Format where

import ColnDo.Common
import Control.Monad (forM_)

formatRules :: Rules ()
formatRules = do
  phony "format-haskell" $ do
    hsFiles <- getHsFiles
    putInfo ("Formatting:" <> mconcat (("\n - " ++) <$> hsFiles))
    cmd_ "fourmolu --mode inplace" hsFiles

  phony "format-cabal" $ do
    projects <- getProjects
    forM_ projects $ \p ->
      cmd_ "cabal-gild --io" (p </> takeFileName p -<.> "cabal")

  phony "format-rust" $ do
    -- XXX: uncomment once we have some rust
    -- cmd_ "cargo fmt"
    pure ()

  phony "format" $ do
    need ["format-haskell", "format-cabal", "format-rust"]

  phony "check-format-haskell" $ do
    hsFiles <- getHsFiles
    putInfo "Checking formatting"
    cmd_ "fourmolu --mode check --indentation 2" hsFiles
