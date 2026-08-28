module Coln.MIR.Realm where

import Coln.Common
import Coln.Core.Params
import Coln.Core.Value qualified as CoreV
import Coln.MIR.Memoed qualified as M
import Coln.MIR.Params
import Coln.MIR.Value qualified as V

data Generator
  = Rel (SUniverse Set Theory) (Bwd Name) (Bwd (V.Ty Set))
  | Fun (Bwd Name) (Bwd (V.Ty Set)) (V.Ty Set)

data RealmDefinition = RealmDefinition
  { body :: M.El Theory
  , ty :: V.Ty Theory
  }

data Realm = Realm
  { root :: V.El Theory
  , rootType :: CoreV.Ty N
  , generators :: Trie Generator
  , realmDefinitions :: OMap Name RealmDefinition
  }
