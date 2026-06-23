-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Core.Globals where

import Data.Map.Ordered (OMap)
import Data.Map.Ordered qualified as OMap

import Coln.Common
import Coln.Core.Params
import Coln.Core.Realm
import Coln.Core.Syntax qualified as S
import Coln.Core.Value qualified as V

data GlobalEntry = GlobalEntry
  { syn :: S.El D
  , val :: V.El N
  , ty :: V.Ty N
  }

data Globals = Globals
  { entries :: OMap Name GlobalEntry
  , realms :: OMap Name Realm
  }

emptyGlobals :: Globals
emptyGlobals = Globals OMap.empty OMap.empty

addGlobalEntry :: Name -> GlobalEntry -> Globals -> Globals
addGlobalEntry n e g = g{entries = g.entries OMap.>| (n, e)}

addRealm :: Name -> Realm -> Globals -> Globals
addRealm n r g = g{realms = g.realms OMap.>| (n, r)}

instance Lookup Globals Name GlobalEntry where
  lookup gs x = OMap.lookup x gs.entries

instance ToList Globals (Name, GlobalEntry) where
  toList ge = OMap.assocs ge.entries
