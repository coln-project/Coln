-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Elaborator.Rules.Variable where

import Prettyprinter
import Prelude hiding (lookup)

import Coln.Common
import Coln.Core
import Coln.Elaborator.Diagnostics
import Coln.Elaborator.Environment
import Coln.Elaborator.Judgment

find :: Span -> Name -> Syn N
find sp x = Syn \e -> do
  (ty, tm, m) <- case lookup e.scope x of
    Just (i, v, ty, m) -> pure (ty, localVar i v, m)
    Nothing -> case lookup e.globals x of
      Just ge -> pure (ge.ty, globalVar x ge.val, ge.mode)
      Nothing -> do
        let msg = "no such variable" <+> dpretty x <+> "in scope"
        failWith e.diagEnv sp VariableNotInScope msg
  case (m, e.scope.mode) of
    (Inductive, Conjunctive) -> do
      let msg = "cannot use inductively bound variable in a conjunctive context"
      failWith e.diagEnv sp InductiveInConjunctive msg
    _ -> pure (ty, tm)
