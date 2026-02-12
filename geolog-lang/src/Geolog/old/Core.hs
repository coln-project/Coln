-- Idea: instead of parameterizing the core by level, make it possible
-- to do a O(1) lookup of the level of any syntax/value
module Geolog.Core where

import Geolog.Common
import Prettyprinter

class PartialOrd a where
  leq :: a -> a -> Bool

data Level
  = Prop
  | Query
  | Theory
  | Top
  | Prim
  deriving (Eq, Show)

instance Pretty Level where
  pretty = pretty . show

instance PartialOrd Level where
  leq l1 l2 = case (l1, l2) of
    (Prop, Prim) -> False
    (Prop, _) -> True
    (Query, Prim) -> False
    (Query, Prop) -> False
    (Query, _) -> True
    (Theory, Prim) -> False
    (Theory, Prop) -> False
    (Theory, Query) -> False
    (Theory, _) -> True
    (Top, Top) -> True
    (Top, _) -> False
    (Prim, Prim) -> True
    (Prim, Top) -> True
    (Prim, _) -> False

class LevelOf a where
  levelOf :: a -> Level

data Universe
  = PropU
  | QueryU
  | PrimU
  | TheoryU
  deriving (Eq, Show)

decodesInto :: Universe -> Level
decodesInto = \case
  PropU -> Prop
  QueryU -> Query
  PrimU -> Prim
  TheoryU -> Theory

codesInto :: Universe -> Level
codesInto = \case
  PropU -> Theory
  QueryU -> Theory
  PrimU -> Top
  TheoryU -> Top

universeFor :: Level -> Maybe Universe
universeFor = \case
  Prop -> Just PropU
  Query -> Just QueryU
  Theory -> Just TheoryU
  Prim -> Just PrimU
  Top -> Nothing

data PiVariant
  = QueryTheory
  | PrimTheory
  | TopTop
  deriving (Eq, Show)

instance Pretty PiVariant where
  pretty = pretty . show

instance LevelOf PiVariant where
  levelOf = \case
    QueryTheory -> Theory
    PrimTheory -> Theory
    TopTop -> Top

piVariant :: Level -> Level -> PiVariant
piVariant l1 l2
  | leq l1 Query && leq l2 Theory = QueryTheory
  | leq l1 Prim && leq l2 Theory = PrimTheory
  | otherwise = TopTop

class Codomain a b | a -> b where
  codomain :: a -> b

instance Codomain PiVariant Level where
  codomain = \case
    QueryTheory -> Theory
    PrimTheory -> Theory
    TopTop -> Top

data Abs a = Abs QName a

data Fields a = Fields [(QName, a)]
  deriving (Functor)

instance ElemAt (Fields a) QName a where
  elemAt (Fields fs) x = go fs
   where
    go [] = impossible
    go ((x', v) : rest)
      | x == x' = v
      | otherwise = go rest

data ElS
  = Var BId
  | Code TyS
  | App ElS ElS
  | Lam (Abs ElS)
  | Proj ElS QName
  | Cons (Fields ElS)

data TyS
  = U Universe
  | Decode Universe ElS
  | Pi PiVariant TyS (Abs TyS)
  | Record Level (Fields TyS)

instance LevelOf TyS where
  levelOf = \case
    U u -> codesInto u
    Decode u _ -> decodesInto u
    Pi pv _ _ -> levelOf pv
    Record l _ -> l

type Env = Bwd ElV

data Clo a = Clo Env QName a

data Spine
  = SId
  | SApp Spine ElV
  | SProj Spine QName

data ElV
  = VNeu FId Spine
  | VCode TyV
  | VLam (Clo ElS)
  | VCons (Fields ElV)

data TyV
  = VU Universe
  | VDecode Universe ElV
  | VPi PiVariant TyV (Clo TyS)
  | VRecord Level Env (Fields TyS)

instance LevelOf TyV where
  levelOf = \case
    VU u -> codesInto u
    VDecode u _ -> decodesInto u
    VPi pv _ _ -> levelOf pv
    VRecord l _ _ -> l
