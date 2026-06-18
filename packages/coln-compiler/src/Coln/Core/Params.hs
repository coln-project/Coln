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

data Sort
  = Set
  | Theory
  | Top
  deriving (Eq, Show)

instance DPretty Sort where
  dpretty = pretty . show

instance PartialOrd Sort where
  leq l1 l2 = case (l1, l2) of
    (Set, _) -> True
    (Theory, Set) -> False
    (Theory, _) -> True
    (Top, Top) -> True
    (Top, _) -> False

maxSort :: Sort -> Sort -> Sort
maxSort l1 l2
  | leq l1 l2 = l2
  | otherwise = l1

data HLevel
  = HProp
  | HSet
  deriving (Eq, Show)

instance PartialOrd HLevel where
  leq l1 l2 = case (l1, l2) of
    (HProp, _) -> True
    (HSet, HProp) -> False
    (HSet, _) -> True

data Level = Level {
  sort :: Sort,
  hlevel :: HLevel }
  deriving (Eq, Show)

instance PartialOrd Level where
  leq (Level s1 h1) (Level s2 h2) = s1 `leq` s2 && h1 `leq` h2

maxLevel :: Level -> Level -> Level
maxLevel l1 l2
  | leq l1 l2 = l2
  | otherwise = l1

class LevelOf a where
  levelOf :: a -> Level

data Universe
  = PropU
  | SetU
  | TheoryU
  | PropTheoryU -- TODO: better name?
  deriving (Eq, Show)

decodesInto :: Universe -> Level
decodesInto = \case
  PropU -> Level Set HProp
  SetU -> Level Set HSet
  PropTheoryU -> Level Theory HProp
  TheoryU -> Level Theory HSet

codesInto :: Universe -> Level
codesInto = \case
  PropU -> Level Theory HSet
  SetU -> Level Theory HSet
  TheoryU -> Level Top HSet
  PropTheoryU -> Level Top HSet

instance Pretty Universe where
  pretty = \case
    PropU -> "Prop"
    SetU -> "Set"
    TheoryU -> "Theory"
    PropTheoryU -> "PropTheory"

universeFor :: Level -> Maybe Universe
universeFor = \case
  Level Set HProp -> Just PropU
  Level Set HSet -> Just SetU
  Level Theory HProp -> Just PropTheoryU
  Level Theory HSet -> Just TheoryU
  Level Top _ -> Nothing

data FunctionVariant
  = SetPropTheory -- TODO: better name?
  | SetTheory
  | TheoryTop
  deriving (Eq, Show)

instance Pretty FunctionVariant where
  pretty = pretty . show

instance LevelOf FunctionVariant where
  levelOf = \case
    SetPropTheory -> Level Theory HProp
    SetTheory -> Level Theory HSet
    TheoryTop -> Level Top HSet

class HasCodomain a b | a -> b where
  codOf :: a -> b

instance HasCodomain FunctionVariant Sort where
  codOf = \case
    SetPropTheory -> Theory
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
