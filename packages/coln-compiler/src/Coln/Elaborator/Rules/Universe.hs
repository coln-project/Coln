-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Elaborator.Rules.Universe where

import Coln.Common
import Coln.Core
import Coln.Core.Value qualified as V
import Coln.Elaborator.Judgment

formation :: Universe -> Typ N
formation u = Typ \_ -> pure $ univ u

intro :: (V.HasEvaluation c) => Span -> Typ c -> Chk c
intro sp t = Chk $ \e ty -> do
  raw <- t.elab e
  case V.behavior ty of
    V.LikeU u -> do
      case leq (levelOf raw) (decodesInto u) of
        True -> pure $ code raw
        False -> do
          let msg = "type" <+> prtIn e raw <+> "too large for universe" <+> pretty u
          failWith e.diagEnv sp TypeTooLarge msg
    _ -> do
      let msg = "cannot check type" <+> prtIn e raw <+> "at non-universe type" <+> prtIn e.scope ty
      failWith e.diagEnv sp TypeAtNonUniverse msg

elim :: Universe -> Chk N -> Typ N
elim u c = Typ \e -> do
  el <- c.elab e $ V.U u
  pure $ decode el

elimSyn :: Span -> Syn N -> Typ N
elimSyn sp s = Typ \e -> do
  (a, el) <- s.elab e
  case V.behavior a of
    V.LikeU u -> pure $ decode el
    _ -> do
      let msg = "expected element of universe type"
      failWith e.diagEnv sp TypeAtNonUniverse msg
