-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.FLIR.Top where

import Coln.Common
import Coln.Core.Params
import Coln.FLIR.Flatten
import Coln.FLIR.Value qualified as FLIR
import Coln.SIR.Realm qualified as SIR

import Data.Map.Ordered qualified as OMap

trieToOMap :: Trie a -> OMap TableName a
trieToOMap t = OMap.fromList [(tableName k, v) | (k, v) <- toList t]

sirToFLIR :: SIR.Realm -> FLIR.Realm
sirToFLIR r =
  FLIR.Realm
    { entities = trieToOMap $ fmap flattenEntity r.entities
    , definitions = trieToOMap $ fmap flattenDefinition r.definitions
    , rules = trieToOMap $ fmap flattenRule r.rules
    }
