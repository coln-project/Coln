module ColnDo.Self where

import ColnDo.Common

selfRules :: Rules ()
selfRules = do
  phony "install-self" $ do
    cmd_ "cabal build coln-do"
    (StdoutTrim (colnDoBin :: String)) <- cmd "cabal list-bin coln-do"
    cmd_ "mkdir -p bin"
    cmd_ "ln -sf" colnDoBin "bin/cdo"
