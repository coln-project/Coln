-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Elaborator.Rules.Equality where

import Coln.Common
import Coln.Core
import Coln.Core.Syntax qualified as S
import Coln.Elaborator.Judgment

formation :: Span -> Syn N -> Syn N -> Typ N
formation sp lhs rhs = Typ \e -> do
  (lty, elhs) <- lhs.elab e
  (rty, erhs) <- rhs.elab e
  case defEq (shape e) lty rty of
    Left err -> do
      let msg = "types" <+> prtIn e lty <+> "and" <+> prtIn e rty <+> "of compared terms differ"
      let note = Just $ dpretty err
      failWithNote e.diagEnv sp TypeMismatch msg note
    Right _ -> case (levelOf lty).mlevel of
      Set -> pure $ equality $ S.EqualityType (fromVTy e.scope.len lty) elhs erhs
      _ -> do
        let msg = "equality requires data types, but" <+> prtIn e lty <+> "is a schema-level type"
        failWith e.diagEnv sp EqualityUnsupported msg
