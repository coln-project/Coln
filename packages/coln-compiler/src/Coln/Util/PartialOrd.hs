-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Util.PartialOrd where

class PartialOrd a where
  leq :: a -> a -> Bool
