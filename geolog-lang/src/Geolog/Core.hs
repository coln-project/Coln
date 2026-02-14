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

type K = Kinetic
type P = Potential

data ElS :: Energy -> Type where
  Var :: BId -> ElS K
  GlobalVar :: Constant -> ElS K
  Code :: (TyS e) -> ElS e
  Lam :: Abs (ElS e) -> ElS e
  App :: ElS e -> ElS K -> ElS e
  Cons :: Fields (ElS e) -> ElS e
  Proj :: ElS e -> QName -> ElS e
  TyRefl :: TyS K -> ElS K
  TmRefl :: ElS K -> TyS K -> ElS K
  Coerce ::
    TyS K -> -- S type
    ElS K -> -- a : S
    TyS K -> -- T type
    ElS K -> -- S = T
    ElS K -- T
  Subst ::
    TyS K -> -- S type
    Abs (TyS K) -> -- (x : S) |- T type
    ElS K -> -- a0 : S
    ElS K -> -- a1 : S
    ElS K -> -- [a0 : S = a1 : S]
    ElS K -- T a0 = T a1
  Coh ::
    TyS K -> -- S type
    ElS e -> -- a : S
    TyS K -> -- T type
    ElS K -> -- S = T
    ElS e -- [a : S = coe a : T]
  Irrel :: ElS K

data TyS :: Energy -> Type where
  U :: Universe -> TyS e
  Decode :: Universe -> ElS e -> TyS e
  Pi :: PiVariant -> TyS K -> Abs (TyS K) -> TyS e
  Record :: Level -> (Fields (TyS K)) -> TyS P
  TyEq :: TyS K -> TyS K -> TyS K
  TmEq :: ElS K -> TyS K -> ElS K -> TyS K -> TyS K

type Env = Bwd (ElV K)

data Clo a = Clo QName (ElV K -> a)

data Spine
  = SId
  | SApp Spine (ElV K)
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
  | VCoe (ElV K) (TyV K) (TyV K)

data Neutral = Neutral
  { head :: Head
  , spine :: Spine
  , behavesAs :: ~(BehavesAs (ElV P))
  , ty :: ~(TyV K)
  }

data ElV :: Energy -> Type where
  VNeu :: Neutral -> ElV K
  VCode :: TyV e -> ElV e
  VLam :: Clo (ElV e) -> ElV e
  VCons :: Fields (ElV e) -> ElV e
  VIrrel :: ElV e

data TeleV a = TVNil | TVCons a (ElV K -> TeleV a)

data FieldsV a = FieldsV [QName] (TeleV a)

data TyV :: Energy -> Type where
  VU :: Universe -> TyV e
  VDecode :: Universe -> Neutral -> TyV K
  VPi :: PiVariant -> TyV K -> Clo (TyV K) -> TyV e
  VRecord :: Level -> FieldsV (TyV K) -> TyV P
  VTyEq :: TyV K -> TyV K -> TyV K
  VTmEq :: ElV K -> TyV K -> ElV K -> TyV K -> TyV K

data GlobalEntry
  = KEntry (ElS K) (ElV K) (TyV K)
  | PEntry (ElS P) (ElV P) (TyV K)

newtype GlobalEnv = GlobalEnv (Map Constant GlobalEntry)

instance ElemAt GlobalEnv Constant GlobalEntry where
  elemAt (GlobalEnv m) c = m Map.! c
