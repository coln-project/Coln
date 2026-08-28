module Coln.SIR.Realm where

import Coln.Common
import Coln.Core.Params
import Coln.Core.Value qualified as CoreV
import Coln.SIR.Syntax

import Data.Aeson qualified as AE
import GHC.Generics

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
  deriving (Show, Eq, Generic)

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

-- JSON
--------------------------------------------------------------------------------

instance AE.ToJSON RuleVariant where
  toEncoding = AE.genericToEncoding aeOptions
