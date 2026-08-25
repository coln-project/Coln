module Coln.SIR.Realm where

import Coln.Common
import Coln.Core.Params
import Coln.SIR.Syntax
import Coln.Core.Value qualified as CoreV

data EntityVariant
  = Table
  | View

data Entity = Entity
  { entityVariant :: EntityVariant
  , columns :: [(Name, Shape)]
  , primaryKey :: Maybe [Int]
  }

data Definition = Definition
  { inCtx :: [(Name, Query)]
  , definand :: TableName
  , args :: [El Set]
  }

data RuleVariant = Enforced | Monitored

data RuleContextSide = Antecedent | Consequent

data Rule = Rule
  { ruleVariant :: RuleVariant
  , ctxSide :: RuleContextSide
  , inCtx :: [(Name, Query)]
  , antecedent :: Prop
  , consequent :: Prop
  }

data Realm = Realm
  { entities :: Trie Entity
  , definitions :: Trie Definition
  , rules :: Trie Rule
  , root :: El Theory
  , rootType :: CoreV.Ty N
  }
