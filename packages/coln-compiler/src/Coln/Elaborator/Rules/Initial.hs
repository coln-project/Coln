module Coln.Elaborator.Rules.Initial where

import Prelude hiding (init)

import Coln.Common
import Coln.Core
import Coln.Elaborator.Environment
import Coln.Elaborator.Judgment

create :: Span -> Typ N -> Syn D
create sp t = Syn \e -> do
  case e.scope.mode of
    Inductive -> pure ()
    Conjunctive -> do
      let msg = "cannot create initial model in conjunctive mode"
      failWith e.diagEnv sp InitInConjunctive msg
  a <- t.elab (e { scope = lock e.scope, target = TargetAnonymous })
  pure (a.val, init a)
  
