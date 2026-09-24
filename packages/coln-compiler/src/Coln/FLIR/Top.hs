-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.FLIR.Top where

import Coln.FLIR.Flatten
import Coln.FLIR.Value qualified as FLIR
import Coln.SIR.Realm qualified as SIR

sirToFLIR :: SIR.Realm -> FLIR.Realm
sirToFLIR r =
  FLIR.Realm
    { entities = fmap flattenEntity r.entities
    , definitions = fmap flattenDefinition r.definitions
    , rules = fmap flattenRule r.rules
    }
