-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT
{-# LANGUAGE TypeAbstractions #-}

module Coln.Elaborator.Judgment (
  module Coln.Elaborator.Diagnostics,
  module Coln.Elaborator.Environment,
  Typ (..),
  Syn (..),
  Chk (..),
  Judgment (..),
  useIs,
)
where

import Coln.Common
import Coln.Core
import Coln.Core.Memoed qualified as M
import Coln.Core.Value qualified as V
import Coln.Elaborator.Diagnostics
import Coln.Elaborator.Environment

newtype Typ c = Typ {elab :: ElabEnv c -> IO (M.Ty c)}

newtype Syn c = Syn {elab :: ElabEnv c -> IO (V.Ty N, M.El c)}

newtype Chk c = Chk {elab :: ElabEnv c -> V.Ty N -> IO (M.El c)}

data Judgment c where
  FromTyp :: Typ c -> Judgment c
  FromSyn :: Syn c -> Judgment c
  FromChk :: DDoc -> Chk c -> Judgment c

useIs :: (V.HasEvaluation c, Functor f) => (ElabEnv N -> f (M.El N)) -> ElabEnv c -> f (M.El c)
useIs @c f e = fmap change $ f e{target = TargetAnonymous}
 where
  change = case V.scase @c of
    SNominative -> id
    SDescriptive -> M.is

annotate :: Typ N -> Chk c -> Syn c
annotate t c = Syn \e -> do
  a <- t.elab (e{target = TargetAnonymous})
  m <- c.elab e a.val
  pure (a.val, m)
