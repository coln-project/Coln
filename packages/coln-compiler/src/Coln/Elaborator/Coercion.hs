module Coln.Elaborator.Coercion where

import Coln.Common
import Coln.Core
import Coln.Core.Memoed qualified as M
import Coln.Core.Value qualified as V
import Coln.Elaborator.Judgment
import Coln.Elaborator.Rules.Universe qualified as Universe
import Coln.Elaborator.Rules.Polarity qualified as Polarity

intoTyp :: Span -> Judgment N -> Typ N
intoTyp _ (FromTyp t) = t
intoTyp sp (FromSyn s) = Universe.elimSyn sp s
intoTyp _ (FromChk _ c) = Universe.elim TheoryU c

intoSyn :: (V.HasEvaluation c) => DDoc -> Span -> Judgment c -> Syn c
intoSyn _ sp (FromTyp t) = Syn $ \e -> do
  raw <- t.elab e
  case universeFor (levelOf raw) of
    Nothing -> do
      let msg = "type" <+> prtIn e raw <+> "too large to fit in a universe"
      failWith e.diagEnv sp TypeTooLarge msg
    Just u -> pure (V.U u, M.code raw)
intoSyn _ _ (FromSyn s) = s
intoSyn use sp (FromChk nd _) = Syn $ \e -> do
  let msg = "Type annotation required when using a" <+> nd <+> "as" <+> use
  failWith e.diagEnv sp AnnotationRequired msg

intoChk :: (V.HasEvaluation c) => Span -> Judgment c -> Chk c
intoChk sp (FromTyp t) = Universe.intro sp t
intoChk sp (FromSyn s) = Polarity.conv sp s
intoChk _ (FromChk _ c) = c
