-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Core.Globals where

import Data.Map.Ordered (OMap)
import Data.Map.Ordered qualified as OMap

import Coln.Common
import Coln.Core.Memoed qualified as M
import Coln.Core.Params
import Coln.Core.Syntax qualified as S
import Coln.Core.Value qualified as V

-- Definitions
--------------------------------------------------------------------------------

data Definition (s :: DefinitionScope) = Definition
  { body :: M.El D
  , ty :: V.Ty N
  , reflected :: V.El N
  , mode :: Mode
  }

mkDefinition :: Name -> V.Ty N -> M.El D -> Mode -> Definition s
mkDefinition x ty tm m = do
  let neu = V.reflect (V.GlobalVar x neu) V.Id ty (Just tm.val)
  Definition tm ty neu m

-- Realms
--------------------------------------------------------------------------------

data Generator
  = Rel [Name] [S.Ty N]
  | Fun [Name] [S.Ty N] (S.Ty N)

data Realm = Realm
  { generators :: Trie Generator
  , root :: V.El N
  , rootType :: V.Ty N
  , realmDefinitions :: OMap Name (Definition Local)
  }

-- Global environment
--------------------------------------------------------------------------------

data Globals = Globals
  { definitions :: OMap Name (Definition Global)
  , realms :: OMap Name Realm
  }

emptyGlobals :: Globals
emptyGlobals = Globals OMap.empty OMap.empty

addDefinition :: Name -> Definition Global -> Globals -> Globals
addDefinition n e g = g{definitions = g.definitions OMap.>| (n, e)}

addRealm :: Name -> Realm -> Globals -> Globals
addRealm n r g = g{realms = g.realms OMap.>| (n, r)}

instance Lookup Globals Name (Definition Global) where
  lookup gs x = OMap.lookup x gs.definitions

instance ToList Globals (Name, Definition Global) where
  toList ge = OMap.assocs ge.definitions
