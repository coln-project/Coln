module Geolog.Core where

import Data.Kind (Type)
import Data.Map.Strict (Map)
import Data.Map.Strict qualified as Map
import Geolog.Common
import Prettyprinter

data Level
  = Query
  | Theory
  | Top
  deriving (Eq, Show)

instance Pretty Level where
  pretty = pretty . show

instance PartialOrd Level where
  leq l1 l2 = case (l1, l2) of
    (Query, _) -> True
    (Theory, Query) -> False
    (Theory, _) -> True
    (Top, Top) -> True
    (Top, _) -> False

class LevelOf a where
  levelOf :: a -> Level

data Universe
  = QueryU
  | TheoryU
  deriving (Eq, Show)

decodesInto :: Universe -> Level
decodesInto = \case
  QueryU -> Query
  TheoryU -> Theory

codesInto :: Universe -> Level
codesInto = \case
  QueryU -> Theory
  TheoryU -> Top

universeFor :: Level -> Maybe Universe
universeFor = \case
  Query -> Just QueryU
  Theory -> Just TheoryU
  Top -> Nothing

-- NOTE: we could potentially replace TheoryTop with TopTop, which would allow
-- for higher-order stuff. Not sure whether this is semantically good though? Or
-- whether this would mess with things we care about...
data PiVariant
  = QueryTheory
  | TheoryTop
  deriving (Eq, Show)

instance Pretty PiVariant where
  pretty = pretty . show

instance LevelOf PiVariant where
  levelOf = \case
    QueryTheory -> Theory
    TheoryTop -> Top

piVariant :: Level -> Level -> Maybe PiVariant
piVariant dom cod
  | leq dom Query && leq cod Theory = Just QueryTheory
  | leq dom Theory = Just TheoryTop
  | otherwise = Nothing

class HasCodomain a b | a -> b where
  codOf :: a -> b

instance HasCodomain PiVariant Level where
  codOf = \case
    QueryTheory -> Theory
    TheoryTop -> Top

data Abs a = Abs QName a | Const a

data Fields a = FNil | FCons QName a (Fields a)
  deriving (Functor)

instance ElemAt (Fields a) QName a where
  elemAt FNil _ = impossible
  elemAt (FCons x' v fs) x
    | x == x' = v
    | otherwise = elemAt fs x

type data Energy = Kinetic | Potential

data ElS :: Energy -> Type where
  Var :: BId -> ElS Kinetic
  GlobalVar :: Constant -> ElS Kinetic
  Code :: (TyS e) -> ElS e
  Lam :: Abs (ElS e) -> ElS e
  App :: ElS e -> ElS Kinetic -> ElS e
  Cons :: Fields (ElS e) -> ElS e
  Proj :: (ElS e) -> QName -> ElS e

data TyS :: Energy -> Type where
  U :: Universe -> TyS e
  Decode :: Universe -> ElS e -> TyS e
  Pi :: PiVariant -> TyS Kinetic -> Abs (TyS Kinetic) -> TyS e
  Record :: Level -> (Fields (TyS Kinetic)) -> TyS Potential

type Env = Bwd (ElV Kinetic)

data Clo a b = Clo Env QName a | VConst b

data Spine
  = SId
  | SApp Spine (ElV Kinetic)
  | SProj Spine QName

data BehavesAs a
  = BehavesAs a
  | TrueNeutral
  deriving (Functor)

data Constant = Constant {name :: QName}
  deriving (Eq, Ord)

data Head
  = Local FId
  | Global Constant

data Neutral = Neutral
  { head :: Head
  , spine :: Spine
  , behavesAs :: BehavesAs (ElV Potential)
  , ty :: ~(TyV Kinetic)
  }

data ElV :: Energy -> Type where
  VNeu :: Neutral -> ElV Kinetic
  VCode :: TyV e -> ElV e
  VLam :: Clo (ElS e) (ElV e) -> ElV e
  VCons :: Fields (ElV e) -> ElV e

data TyV :: Energy -> Type where
  VU :: Universe -> TyV e
  VDecode :: Universe -> Neutral -> TyV Kinetic
  VPi :: PiVariant -> TyV Kinetic -> Clo (TyS Kinetic) (TyV Kinetic) -> TyV e
  VRecord :: Level -> Env -> Fields (TyS Kinetic) -> TyV Potential

data GlobalEntry
  = KineticEntry (ElS Kinetic) (ElV Kinetic) (TyV Kinetic)
  | PotentialEntry (ElS Potential) (ElV Potential) (TyV Kinetic)

newtype GlobalEnv = GlobalEnv (Map Constant GlobalEntry)

instance ElemAt GlobalEnv Constant GlobalEntry where
  elemAt (GlobalEnv m) c = m Map.! c
