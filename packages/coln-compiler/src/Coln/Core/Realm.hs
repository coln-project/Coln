module Coln.Core.Realm where

import Coln.Common
import Coln.Core.Params
import Coln.Core.Syntax qualified as S

data Generator
  = Rel [Name] [S.Ty N]
  | Fun [Name] [S.Ty N] (S.Ty N)

-- Generator trie
data GenTrie
  = Leaf Generator
  | Node (Dict GenTrie)

data Realm = Realm
  { generators :: GenTrie }
