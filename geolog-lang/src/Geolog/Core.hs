module Geolog.Core where

import Data.Singletons.TH

import Geolog.Common
import Data.Kind (Type)

data Abs f l = Abs QName (f l)

$(singletons [d| data Level = Sort | Th | Set | Prim |])

data Any :: (Level -> Type) -> Type where
  Any :: Sing l -> f l -> Any f

extract :: forall l f. SingI l => Any f -> f l
extract (Any s a) = case (s, sing :: Sing l) of
  (SSort, SSort) -> a
  (STh, STh) -> a
  (SSet, SSet) -> a
  (SPrim, SPrim) -> a
  _ -> error "tried to extract at a non-matching level"

data LevelInclusion :: Level -> Level -> Type where
  SortInTh :: LevelInclusion Sort Th
  SortInSet :: LevelInclusion Sort Set
  ThInSet :: LevelInclusion Th Set
  PrimInSet :: LevelInclusion Prim Set

withDom :: LevelInclusion l l' -> (SingI l => a) -> a
withDom SortInTh x = x
withDom SortInSet x = x
withDom ThInSet x = x
withDom PrimInSet x = x

data ElS :: Level -> Type where
  Var :: BId -> ElS l
  SortCode :: TyS Sort -> ElS Th
  ThCode :: TyS Th -> ElS Set
  ThApp :: ElS Th -> ElS Sort -> ElS Th
  ThLam :: Abs ElS Th -> ElS Th
  SetApp :: ElS Set -> ElS Set -> ElS Set
  SetLam :: Abs ElS Set -> ElS Set
  Proj :: ElS l -> QName -> ElS l
  Cons :: Fields ElS l -> ElS l
  LiftEl :: ElS l -> LevelInclusion l l' -> ElS l'

data Fields f l = Fields [(QName, f l)]

data TyS :: Level -> Type where
  SortU :: TyS Th
  SortEl :: ElS Th -> TyS Sort
  ThU :: TyS Set
  ThEl :: ElS Set -> TyS Th
  ThPi :: TyS Sort -> Abs TyS Th -> TyS Th
  SetPi :: TyS Set -> Abs TyS Set -> TyS Set
  Record :: Fields TyS l -> TyS l
  LiftTy :: TyS l -> LevelInclusion l l' -> TyS l'

-- Whenever we use an element value, we should already know what type it is.
-- So we shouldn't need to redundantly encode it.

-- We need to be able to quote back values to syntax, so we actually do need
-- a good amount of information in the values.

-- We're going to defunctionalize for ease of debugging.

type Env = Bwd (Any ElV)

data Clo f l = Clo Env QName (f l)

data Sp :: Level -> Type where
  SId :: Sp l
  SThApp :: Sp Th -> ElV Sort -> Sp Th
  SSetApp :: Sp Set -> ElV Set -> Sp Set
  SProj :: Sp l -> QName -> Sp l

data ElV :: Level -> Type where
  VNeu :: FId -> Sp l -> ElV l
  VSortCode :: TyV Sort -> ElV Th
  VThCode :: TyV Th -> ElV Set
  VLiftEl :: ElV l -> LevelInclusion l l' -> ElV l'
  VThLam :: Clo ElS Th -> ElV Th
  VSetLam :: Clo ElS Set -> ElV Set
  VCons :: Fields ElV l -> ElV l

data TyV :: Level -> Type where
  VSortU :: TyV Th
  VSortEl :: ElV Th -> TyV Sort
  VThU :: TyV Set
  VThEl :: ElV Set -> TyV Th
  VThPi :: TyV Sort -> Clo TyS Th -> TyV Th
  VSetPi :: TyV Set -> Clo TyS Set -> TyV Set
  VRecord :: Env -> Fields TyS l -> TyV l
  VLiftTy :: TyV l -> LevelInclusion l l' -> TyV l'
