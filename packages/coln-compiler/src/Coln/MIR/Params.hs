module Coln.MIR.Params where

import Coln.Common
import Coln.Core.Params

data SMLevel :: MLevel -> Type where
  SSet :: SMLevel Set
  STheory :: SMLevel Theory
  STop :: SMLevel Top

withLevel :: MLevel -> (forall l. SMLevel l -> a) -> a
withLevel l f = case l of
  Set -> f SSet
  Theory -> f STheory
  Top -> f STop

data SLevel (l :: MLevel) = SLevel
  { mlevel :: SMLevel l
  , hlevel :: HLevel
  }

class LevelCoerce (f :: MLevel -> Type) where
  levelCoerce :: SMLevel l0 -> SMLevel l1 -> f l0 -> f l1

levelCoerceFromMatch :: (LevelCoerce f) => SMLevel l -> Match SMLevel f -> f l
levelCoerceFromMatch l1 (Pair l0 v) = levelCoerce l0 l1 v

data Lift :: MLevel -> MLevel -> Type where
  LSetTheory :: Lift Set Theory
  LTheoryTop :: Lift Theory Top

data SUniverse :: MLevel -> MLevel -> Type where
  SSetU :: SUniverse Set Theory
  SPropU :: SUniverse Set Theory
  STheoryU :: SUniverse Theory Top

sDecodesInto :: SUniverse l0 l1 -> SMLevel l0
sDecodesInto = \case
  SSetU -> SSet
  SPropU -> SSet
  STheoryU -> STheory

sCodesInto :: SUniverse l0 l1 -> SMLevel l1
sCodesInto = \case
  SSetU -> STheory
  SPropU -> STheory
  STheoryU -> STop

withUniverse :: Universe -> (forall l0 l1. SUniverse l0 l1 -> a) -> a
withUniverse u f = case u of
  SetU -> f SSetU
  PropU -> f SPropU
  TheoryU -> f STheoryU

inferSetCodes :: SUniverse l Theory -> SUniverse Set Theory
inferSetCodes SSetU = SSetU
inferSetCodes SPropU = SPropU

data SMFunctionVariant :: MLevel -> MLevel -> Type where
  SSetTheory :: SMFunctionVariant Set Theory
  STheoryTop :: SMFunctionVariant Theory Top

sDom :: SMFunctionVariant l0 l1 -> SMLevel l0
sDom = \case
  SSetTheory -> SSet
  STheoryTop -> STheory

sCod :: SMFunctionVariant l0 l1 -> SMLevel l1
sCod = \case
  SSetTheory -> STheory
  STheoryTop -> STop

withFunctionVariant :: FunctionVariantMLevel -> (forall l0 l1. SMFunctionVariant l0 l1 -> a) -> a
withFunctionVariant fv f = case fv of
  SetTheory -> f SSetTheory
  TheoryTop -> f STheoryTop

data SFunctionVariant (l0 :: MLevel) (l1 :: MLevel) = SFunctionVariant
  { mlevel :: SMFunctionVariant l0 l1
  , hlevel :: HLevel
  }

instance HLevelOf (SFunctionVariant l0 l1) where
  hlevelOf sfv = sfv.hlevel
