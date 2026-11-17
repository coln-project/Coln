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
  | View Materialization
  deriving (Show, Eq, Generic)

data Table = Table
  { tableVariant :: TableVariant
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

emptyGenerators :: Generators
emptyGenerators = Generators OMap.empty OMap.empty OMap.empty OMap.empty

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

emitTable :: Table -> GeneratorWriter ()
emitTable e = do
  p <- ask
  modify (\gs -> gs { tables = gs.tables OMap.>| (manglePath p, e) })

emitRule :: Rule -> GeneratorWriter ()
emitRule d = do
  p <- ask
  modify (\gs -> gs { rules = gs.rules OMap.>| (manglePath p, d) })

emitLaw :: Maybe Name -> Law -> GeneratorWriter ()
emitLaw mx l = do
  p <- ask
  let p' = case mx of
        Just x -> p :> x
        Nothing -> p
  modify (\gs -> gs { laws = gs.laws OMap.>| (manglePath p', l) })

emitUnion :: GeneratorWriter ()
emitUnion = do
  p <- ask
  modify (\gs -> gs { unions = gs.unions OMap.>| (manglePath p, Union BwdNil) })

emitCaseFor :: GlobalSymbol -> GeneratorWriter ()
emitCaseFor tn = do
  p <- ask
  modify $ \gs -> do
   let u = elemAt gs.unions tn
   gs { unions = gs.unions OMap.|> (tn, Union (u.variants :> manglePath p)) }

-- JSON
--------------------------------------------------------------------------------

instance AE.ToJSON LawVariant where
  toEncoding = AE.genericToEncoding aeOptions
