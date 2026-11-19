-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.SIR.Realm where

import Coln.Common
import Coln.Core.Params
import Coln.SIR.Syntax

import Control.Monad.RWS.Strict
import Data.Aeson qualified as AE
import Data.Map.Ordered qualified as OMap
import GHC.Generics


data Materialization
  = Recomputed
  | Materialized
  deriving (Show, Eq, Generic)

data TableVariant
  = Ground
  | Cons
  | Union (Bwd GlobalSymbol)
  | View Materialization
  deriving (Show, Eq, Generic)

newtype PrimaryKey = PrimaryKey Int

data Table = Table
  { tableVariant :: TableVariant
  , columns :: [(Name, Shape)]
  , primaryKey :: Maybe PrimaryKey
  }

data Rule = Rule
  { inCtx :: [(Name, Shape)]
  , antecedent :: [Prop]
  , definand :: GlobalSymbol
  , args :: [El Set]
  }

data LawVariant = Enforced | Monitored
  deriving (Show, Eq, Generic)

data LawContextSide = Antecedent | Consequent

data Law = Law
  { ruleVariant :: LawVariant
  , inCtx :: [(Name, Shape)]
  , antecedent :: [Prop]
  , consequent :: [Prop]
  }

data Generators = Generators
  { tables :: OMap GlobalSymbol Table
  , rules :: OMap GlobalSymbol Rule
  , laws :: OMap GlobalSymbol Law
  }

emptyGenerators :: Generators
emptyGenerators = Generators OMap.empty OMap.empty OMap.empty

data Realm = Realm
  { generators :: Generators
  , auxillaries :: OMap Name (El Theory, TheoryShape)
  }

-- GeneratorWriter
--------------------------------------------------------------------------------

newtype GeneratorWriter a = GeneratorWriter
  { underlying :: RWS Path () Generators a }
  deriving (Functor, Applicative, Monad, MonadReader Path, MonadState Generators)

runGeneratorWriter :: GeneratorWriter a -> (a, Generators)
runGeneratorWriter action = do
  let (result, generators, _) = runRWS action.underlying BwdNil emptyGenerators
  (result, generators)

currentSymbol :: GeneratorWriter GlobalSymbol
currentSymbol = manglePath <$> ask

emitTable :: Table -> GeneratorWriter ()
emitTable t = do
  x <- currentSymbol
  modify (\gs -> gs { tables = gs.tables OMap.>| (x, t) })

emitRule :: Rule -> GeneratorWriter ()
emitRule r = do
  x <- currentSymbol
  modify (\gs -> gs { rules = gs.rules OMap.>| (x, r) })

emitLaw :: Law -> GeneratorWriter ()
emitLaw l = do
  x <- currentSymbol
  modify (\gs -> gs { laws = gs.laws OMap.>| (x, l) })

emitVariantOf :: GlobalSymbol -> GeneratorWriter ()
emitVariantOf s = do
  x <- currentSymbol
  modify $ \gs -> do
    let t = elemAt gs.tables s
    let t' = case t.tableVariant of
          Union variants -> t { tableVariant = Union (variants :> x) }
          _ -> panic "attempted to add a case to a non-union table"
    gs { tables = gs.tables OMap.>| (s, t') }

-- JSON
--------------------------------------------------------------------------------

instance AE.ToJSON LawVariant where
  toEncoding = AE.genericToEncoding aeOptions
