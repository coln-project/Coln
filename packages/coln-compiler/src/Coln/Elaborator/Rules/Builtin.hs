-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Elaborator.Rules.Builtin where

import Coln.Core
import Coln.Core.Value qualified as V
import Coln.Elaborator.Judgment

formation :: BuiltinTy -> Typ N
formation bt = Typ $ \_ -> do
  pure $ builtinTy bt

intro :: Literal -> Syn N
intro l = Syn $ \_ -> case l of
  LitInt _ -> pure (V.BuiltinTy BuiltinInt, lit l)
  LitString _ -> pure (V.BuiltinTy BuiltinString, lit l)
