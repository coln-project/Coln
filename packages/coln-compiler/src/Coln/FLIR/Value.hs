-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT
{-# OPTIONS_GHC -Wno-orphans #-}

module Coln.FLIR.Value where

import Coln.Common
import Coln.Core.Params
import Coln.SIR.Realm qualified as SIR
import Coln.SIR.Syntax qualified as SIR

import GHC.Generics

newtype LocalName = LocalName { name :: Text }
  deriving (Show, Eq, Generic)

type ValType = SIR.ScalarType

data Materialization
  = Recomputed
  | Materialized
  deriving (Show, Eq, Generic)

type TableVariant = SIR.TableVariant

newtype Context = Context { bindings :: [(LocalName, ValType)] }
  deriving (Show, Eq, Generic)

nameAt :: Context -> FId -> LocalName
nameAt c (FId i) = fst $ c.bindings !! i

newtype PhysicalPrimaryKey = PhysicalPrimaryKey { columns :: Int }
  deriving (Show, Eq, Generic)

data Table = Table
  { tableVariant :: TableVariant PhysicalPrimaryKey
  , columns :: Context
  }
  deriving (Show, Eq, Generic)

data El
  = Lit Literal
  | LocalVar FId
  deriving (Show, Eq, Generic)

data Atom = Atom
  { table :: GlobalSymbol
  , rowId :: Maybe El
  , values :: [El]
  }
  deriving (Show, Eq, Generic)

data Prop
  = PAtom Atom
  | PEq ValType El El
  deriving (Show, Eq, Generic)

data Law = Law
  { lawVariant :: SIR.LawVariant
  , context :: Context
  , antecedents :: [Prop]
  , consequents :: [Prop]
  }

data Rule = Rule
  { context :: Context
  , antecedents :: [Prop]
  , consequent :: Atom
  }

data Query = Query
  { context :: Context
  , props :: [Prop]
  }

type Union = SIR.Union

data Realm = Realm
  { tables :: OMap GlobalSymbol Table
  , rules :: OMap GlobalSymbol Rule
  , laws :: OMap GlobalSymbol Law
  }
