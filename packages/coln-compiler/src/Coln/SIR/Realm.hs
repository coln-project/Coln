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
  | Memoized
  | Materialized

data EntityVariant
  = Table
  | View Materialization

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
  { entities :: OMap TableName Entity
  , definitions :: OMap TableName Definition
  , rules :: OMap TableName Rule
  , root :: El Theory
  , rootType :: TheoryShape
  , auxillaries :: OMap Name (El Theory, TheoryShape)
  }

-- JSON
--------------------------------------------------------------------------------

instance AE.ToJSON RuleVariant where
  toEncoding = AE.genericToEncoding aeOptions
