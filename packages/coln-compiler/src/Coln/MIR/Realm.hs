module Coln.MIR.Realm where

import Coln.Common
import Coln.Core.Params
import Coln.MIR.Params
import Coln.MIR.Value qualified as V
import Coln.MIR.Memoed qualified as M

data Generator
  = Rel (SUniverse Set Theory) (Bwd Name) (Bwd (V.Ty Set))
  | Fun (Bwd Name) (Bwd (V.Ty Set)) (V.Ty Set)

data RealmDefinition = RealmDefinition
  { body :: M.El Theory
  , ty :: V.Ty Theory
  }

data Realm = Realm
  { root :: V.El Theory
  , rootType :: V.Ty Theory
  , generators :: Trie Generator
  , realmDefinitions :: OMap Name RealmDefinition
  }
