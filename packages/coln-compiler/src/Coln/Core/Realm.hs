module Coln.Core.Realm where

import Coln.Common
import Coln.Core.Params
import Coln.Core.Syntax qualified as S

data Generator
  = Rel [Name] [S.Ty N]
  | Fun [Name] [S.Ty N] (S.Ty N)

data Realm = Realm
  { generators :: Trie Generator }
