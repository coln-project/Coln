module Coln.Core.Realm where

import Coln.Common
import Coln.Core.Params
import Coln.Core.Syntax qualified as S
import Coln.Core.Value qualified as V

data Generator
  = Rel [Name] [S.Ty N]
  | Fun [Name] [S.Ty N] (S.Ty N)

data Realm = Realm
  { generators :: Trie Generator
  , root :: V.El N
  , rootType :: V.Ty N
  }
