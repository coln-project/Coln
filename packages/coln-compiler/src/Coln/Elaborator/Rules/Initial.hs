module Coln.Elaborator.Rules.Initial where

import Prelude hiding (init)

import Coln.Core
import Coln.Elaborator.Environment
import Coln.Elaborator.Judgment

create :: Typ N -> Syn D
create t = Syn \e -> do
  a <- t.elab (e { scope = lock e.scope, target = TargetAnonymous })
  pure (a.val, init a)
  
