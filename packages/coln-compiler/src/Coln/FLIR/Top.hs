-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.FLIR.Top where

import Coln.FLIR.Flatten
import Coln.FLIR.Value qualified as FLIR
import Coln.SIR.Realm qualified as SIR

sirToFLIR :: SIR.Realm -> FLIR.Realm
sirToFLIR r = do
  let g = r.generators
  FLIR.Realm
    { tables = fmap flattenTable g.tables
    , unions = g.unions
    , rules = fmap flattenRule g.rules
    , laws = fmap flattenLaw g.laws
    }
