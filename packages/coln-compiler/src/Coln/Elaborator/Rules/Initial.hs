module Coln.Elaborator.Rules.Initial where

import Coln.Common
import Coln.Core
import Coln.Elaborator.Diagnostics
import Coln.Elaborator.Environment
import Coln.Elaborator.Judgment

create :: Typ N -> Syn D
create t = Syn \e -> do
  a <- t.elab (e { scope = lock e.scope })
  pure (a, init a)
  
