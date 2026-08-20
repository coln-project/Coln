module Coln.SIR.Realm where

import Coln.Common
import Coln.Core.Params
import Coln.SIR.Syntax

data EntityType
  = Table
  | View

data Entity = Entity
  { entityType :: EntityType
  , columnNames :: [Name]
  , columnShapes :: [Shape]
  , primaryKey :: Maybe [Int]
  }

data Definition = Definition
  { inCtx :: [Query]
  , definand :: TableName
  , args :: [El Set]
  }

data Law = Law
  { inCtx :: [Query]
  , antecedent :: Prop
  , consequent :: Prop
  }

data Realm = Realm
  { entities :: Trie Entity
  , definitions :: Trie Definition
  , laws :: Trie Law
  }
