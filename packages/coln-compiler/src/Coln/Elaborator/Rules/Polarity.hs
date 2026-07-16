module Coln.Elaborator.Rules.Polarity where

import Coln.Common
import Coln.Core
import Coln.Core.Value qualified as V
import Coln.Elaborator.Judgment

conv :: (V.HasEvaluation c) => Span -> Syn c -> Chk c
conv sp s = Chk \e a -> do
  (a', m) <- s.elab e
  case defEq (shape e) a a' of
    Right _ -> pure m
    Left err -> do
      let msg = "expected type" <+> prtIn e.scope a <> ", but got type" <+> prtIn e.scope a'
      let note = Just $ dpretty err
      failWithNote e.diagEnv sp TypeMismatch msg note

annot :: (V.HasEvaluation c) => Chk c -> Typ N -> Syn c
annot c t = Syn \e -> do
  a <- t.elab (e { target = TargetAnonymous })
  m <- c.elab e a.val
  pure (a.val, m)
