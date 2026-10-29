-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.SIR.Realm where

import Coln.Common
import Coln.Core.Params
import Coln.SIR.Syntax

import Data.Aeson qualified as AE
import GHC.Generics


data Materialization
  = Recomputed
  | Materialized

-- Thoughts on what the entity variant should be

-- We could have a Hashcons EntityVariant

-- The big question is around types, I think. Namely, what is the tablename
-- associated with the row id for a syntax tree?

-- Also, we need to explicitly declare the enum of tags, I think

data EntityVariant
  = Table
  | Hashcons
  | View Materialization

data Entity = Entity
  { entityVariant :: EntityVariant
  , columns :: [(Name, Shape)]
  , primaryKey :: Maybe [Int]
  }

data Union = Union
  { variants :: Bwd TableName
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

data Generators = Generators
  { entities :: OMap TableName Entity
  , unions :: OMap TableName Union
  , definitions :: OMap TableName Definition
  , rules :: OMap TableName Rule
  }

data Realm = Realm
  { generators :: Generators
  , auxillaries :: OMap Name (El Theory, TheoryShape)
  }

-- JSON
--------------------------------------------------------------------------------

instance AE.ToJSON RuleVariant where
  toEncoding = AE.genericToEncoding aeOptions
