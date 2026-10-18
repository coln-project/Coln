-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Util.Match where

import Data.Kind (Type)

data Match (f :: k -> Type) (g :: k -> Type) where
  Pair :: f i -> g i -> Match f g
