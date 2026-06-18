module Coln.Backend.IR where

import Data.Set qualified as Set

-- XXX Lit/BultinTy should probably be moved up in the hierarchy
import Coln.Common
import Coln.Core.Params

type ColName = Path

data ColType
  = RowId TableName
  | BuiltinTy BuiltinTy
  deriving (Show, Eq)

data Materialization
  = Recomputed
  | Memoized
  | Materialized
  deriving (Show, Eq)

data IndexMethod
  = BTree

data EntityVariant
  = Table
  | View Materialization
  | Index IndexMethod [ColName]

data Entity = Entity
  { entityVariant :: EntityVariant
  -- , columns :: Trie ColType
  , columns :: [(ColName, ColType)]
  , primaryKey :: Maybe (Set.Set ColName)
  }

data Term
  = Lit Literal
  | Var FId
  deriving (Show, Eq)

data Atom = Atom
  { entity :: TableName
  , rowId :: Maybe Term
  , values :: Map Int Term
  }
  deriving (Show, Eq)

data Prop
  = PAtom Atom
  | PEq Term Term
  deriving (Show, Eq)

data RuleVariant = Chased | Enforced | Monitored

data Rule = Rule
  { ruleVariant :: RuleVariant
  , varNames :: Bwd ColName
  , varTypes :: Bwd ColType
  , antecedents :: [Prop]
  , consequents :: [Prop]
  }

data FlatRealm = FlatRealm
  { entities :: Map TableName Entity
  , rules :: Map TableName Rule
  }
