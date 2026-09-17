-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.MIR.Realm where

import Coln.Common
import Coln.Core.Params
import Coln.MIR.Memoed qualified as M
import Coln.MIR.Params
import Coln.MIR.Value qualified as V

data GenTy
  = GenU (SUniverse Set Theory)
  | GenLift (V.Ty N Set)

data Generator = Generator
  { providence :: Providence
  , paramNames :: Bwd Name
  , paramTypes :: Bwd (V.Ty N Set)
  , codom :: GenTy
  }

data RealmDefinition = RealmDefinition
  { body :: M.El N Theory
  , ty :: V.Ty N Theory
  }

data Realm = Realm
  { root :: V.El N Theory
  , rootType :: V.Ty N Theory
  , generators :: Trie Generator
  , realmDefinitions :: OMap Name RealmDefinition
  }
