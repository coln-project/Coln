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

-- We could have a Hashcons TableVariant

-- The big question is around types, I think. Namely, what is the tablename
-- associated with the row id for a syntax tree?

-- Also, we need to explicitly declare the enum of tags, I think

data TableVariant
  = Ground
  | Cons
  | View Materialization

data Table = Table
  { entityVariant :: TableVariant
  , columns :: [(Name, Shape)]
  , primaryKey :: Maybe [Int]
  }

data Union = Union
  { variants :: Bwd GlobalSymbol
  }

data Rule = Rule
  { inCtx :: [(Name, Query)]
  , definand :: GlobalSymbol
  , args :: [El Set]
  }

data LawVariant = Enforced | Monitored
  deriving (Show, Eq, Generic)

data LawContextSide = Antecedent | Consequent

data Law = Law
  { ruleVariant :: LawVariant
  , ctxSide :: LawContextSide
  , inCtx :: [(Name, Query)]
  , antecedent :: Prop
  , consequent :: Prop
  }

data Generators = Generators
  { tables :: OMap GlobalSymbol Table
  , unions :: OMap GlobalSymbol Union
  , rules :: OMap GlobalSymbol Rule
  , laws :: OMap GlobalSymbol Law
  }

data Realm = Realm
  { generators :: Generators
  , auxillaries :: OMap Name (El Theory, TheoryShape)
  }

-- JSON
--------------------------------------------------------------------------------

instance AE.ToJSON LawVariant where
  toEncoding = AE.genericToEncoding aeOptions
