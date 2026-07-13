-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Elaborator.Rules.Universe where

import Coln.Core
import Coln.Elaborator.Judgment

formation :: Universe -> Typ N
formation u = Typ \_ -> pure $ univ u
