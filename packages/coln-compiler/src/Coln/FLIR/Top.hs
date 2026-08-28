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

trieToOMap :: RealmId -> Trie a -> OMap TableName a
trieToOMap rId t = OMap.fromList [(TableName rId k, v) | (k, v) <- toList t]

sirToFLIR :: RealmId -> SIR.Realm -> FLIR.Realm
sirToFLIR rId r =
  FLIR.Realm
    { entities = trieToOMap rId $ fmap flattenEntity r.entities
    , definitions = trieToOMap rId $ fmap flattenDefinition r.definitions
    , rules = trieToOMap rId $ fmap flattenRule r.rules
    }
