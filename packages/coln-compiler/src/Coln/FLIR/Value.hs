module Coln.FLIR.Value where

import Coln.Common
import Coln.Core.Params
import Coln.SIR.Syntax qualified as SIR

import Data.Set qualified as Set
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
  , primaryKey :: Maybe (Set.Set ColName)
  }
  deriving (Show, Eq, Generic)

-- The type parameter refers to the type of external references In the Realm IR,
-- this is Void, but in the FFI which constructs queries dynamically, this is
-- host-language expressions.
data El e
  = Lit Literal
  | LocalVar FId
  | Extern e
  deriving (Show, Eq, Generic)

data Atom e = Atom
  { entity :: TableName
  , rowId :: Maybe (El e)
  , values :: [Maybe (El e)]
  }

data Prop e
  = PAtom (Atom e)
  | PEq (El e) (El e)

data RuleVariant = Enforced | Monitored

data Rule = Rule
  { ruleVariant :: RuleVariant
  , vars :: [(ColName, ColType)]
  , antecedents :: [Prop Void]
  , consequents :: [Prop Void]
  }
