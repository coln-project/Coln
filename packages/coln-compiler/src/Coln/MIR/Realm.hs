module Coln.MIR.Realm where

import Coln.Common
import Coln.Core.Params
import Coln.MIR.Memoed qualified as M
import Coln.MIR.Params
import Coln.MIR.Value qualified as V

data Providence
  = Holy -- A god-given relation or function, derived from laying out an initial model
  | Profane -- A user-edited relation or function, derived from laying out the root theory

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
