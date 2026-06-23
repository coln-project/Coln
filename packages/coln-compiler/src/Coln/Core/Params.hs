-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT
{-# LANGUAGE DeriveGeneric #-}

module Coln.Core.Params where

import Coln.Common
import GHC.Generics (Generic)
import Prettyprinter

-- Level stuff (levels, universes, function variants)
--------------------------------------------------------------------------------

data Level
  = Set
  | Theory
  | Top
  deriving (Eq, Show)

instance DPretty Level where
  dpretty = pretty . show

instance PartialOrd Level where
  leq l1 l2 = case (l1, l2) of
    (Set, _) -> True
    (Theory, Set) -> False
    (Theory, _) -> True
    (Top, Top) -> True
    (Top, _) -> False

maxLevel :: Level -> Level -> Level
maxLevel l1 l2
  | leq l1 l2 = l2
  | otherwise = l1

class LevelOf a where
  levelOf :: a -> Level

data Universe
  = SetU
  | TheoryU
  deriving (Eq, Show)

decodesInto :: Universe -> Level
decodesInto = \case
  SetU -> Set
  TheoryU -> Theory

codesInto :: Universe -> Level
codesInto = \case
  SetU -> Theory
  TheoryU -> Top

instance Pretty Universe where
  pretty = \case
    SetU -> "Set"
    TheoryU -> "Theory"

universeFor :: Level -> Maybe Universe
universeFor = \case
  Set -> Just SetU
  Theory -> Just TheoryU
  Top -> Nothing

data FunctionVariant
  = SetTheory
  | TheoryTop
  deriving (Eq, Show)

instance Pretty FunctionVariant where
  pretty = pretty . show

instance LevelOf FunctionVariant where
  levelOf = \case
    SetTheory -> Theory
    TheoryTop -> Top

class HasCodomain a b | a -> b where
  codOf :: a -> b

instance HasCodomain FunctionVariant Level where
  codOf = \case
    SetTheory -> Theory
    TheoryTop -> Top

-- Case
--------------------------------------------------------------------------------

type data Case = Nominative | Descriptive

type N = Nominative

type D = Descriptive

data SCase :: Case -> Type where
  SNominative :: SCase Nominative
  SDescriptive :: SCase Descriptive

-- Literals
--------------------------------------------------------------------------------

data Literal
  = LitInt Int
  | LitString Text
  deriving (Show, Eq)

instance Pretty Literal where
  pretty = \case
    LitInt i -> pretty i
    LitString t -> "\"" <> pretty t <> "\""

data BuiltinTy
  = BuiltinInt
  | BuiltinString
  deriving (Eq, Generic)

instance Show BuiltinTy where
  show = \case
    BuiltinInt -> "Int"
    BuiltinString -> "String"

-- Context shape
--------------------------------------------------------------------------------

data CtxShape = CtxShape {len :: Int, names :: Bwd Name}

-- Realms
--------------------------------------------------------------------------------

type RealmId = Name
type Path = Bwd Name

data TableName = TableName {realm :: RealmId, path :: Path}
  deriving (Show, Eq, Ord)

instance DPretty TableName where
  dpretty tn = concatWith (surround dot) (dpretty <$> toList tn.path)
