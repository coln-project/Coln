module Coln.FLIR.Value where

import Coln.Common
import Coln.Core.Params
import Coln.SIR.Realm qualified as SIR
import Coln.SIR.Syntax qualified as SIR

import GHC.Generics

type ColName = Path

type ColType = SIR.ScalarType

data Materialization
  = Recomputed
  | Memoized
  | Materialized
  deriving (Show, Eq, Generic)

data IndexMethod
  = BTree
  deriving (Show, Eq, Generic)

data EntityVariant
  = Table
  | View Materialization
  | Index IndexMethod [ColName]
  deriving (Show, Eq, Generic)

data Entity = Entity
  { entityVariant :: EntityVariant
  , columns :: [(ColName, ColType)]
  , primaryKey :: Maybe [Int]
  }
  deriving (Show, Eq, Generic)

-- Param only shows up in queries, not in the FLIR for a realm
data El
  = Lit Literal
  | LocalVar FId
  | Param FId
  deriving (Show, Eq, Generic)

data Atom = Atom
  { entity :: TableName
  , rowId :: Maybe El
  , values :: [Maybe El]
  }

data Prop
  = PAtom Atom
  | PEq El El

data Rule = Rule
  { ruleVariant :: SIR.RuleVariant
  , vars :: [(ColName, ColType)]
  , antecedents :: [Prop]
  , consequents :: [Prop]
  }

data Definition = Definition
  { vars :: [(ColName, ColType)]
  , antecedents :: [Prop]
  , definand :: TableName
  , args :: [El]
  }

data Query = Query
  { vars :: [(ColName, ColType)]
  , props :: [Prop]
  }
