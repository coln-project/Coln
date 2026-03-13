module Geolog.Core where

import Data.Kind (Type)
import Data.Map.Strict (Map)
import Data.Map.Strict qualified as Map
import Data.Text (Text)
import Geolog.Common
import Prettyprinter

data Level
  = Query
  | Theory
  | Top
  | Prim
  deriving (Eq, Show)

instance Pretty Level where
  pretty = pretty . show

instance PartialOrd Level where
  leq l1 l2 = case (l1, l2) of
    (Query, Prim) -> False
    (Query, _) -> True
    (Theory, Query) -> False
    (Theory, Prim) -> False
    (Theory, _) -> True
    (Top, Top) -> True
    (Top, _) -> False
    (Prim, Top) -> True
    (Prim, Prim) -> True
    (Prim, _) -> False

class LevelOf a where
  levelOf :: a -> Level

data Universe
  = QueryU
  | TheoryU
  | PrimU
  deriving (Eq, Show)

decodesInto :: Universe -> Level
decodesInto = \case
  QueryU -> Query
  TheoryU -> Theory
  PrimU -> Prim

codesInto :: Universe -> Level
codesInto = \case
  QueryU -> Theory
  TheoryU -> Top
  PrimU -> Top

universeFor :: Level -> Maybe Universe
universeFor = \case
  Query -> Just QueryU
  Theory -> Just TheoryU
  Prim -> Just PrimU
  Top -> Nothing

-- NOTE: we could potentially replace TheoryTop with TopTop, which would allow
-- for higher-order stuff. Not sure whether this is semantically good though? Or
-- whether this would mess with things we care about...
data PiVariant
  = QueryTheory
  | PrimTheory
  | TheoryTop
  deriving (Eq, Show)

instance Pretty PiVariant where
  pretty = pretty . show

instance LevelOf PiVariant where
  levelOf = \case
    QueryTheory -> Theory
    PrimTheory -> Theory
    TheoryTop -> Top

piVariant :: Level -> Level -> Maybe PiVariant
piVariant dom cod
  | leq dom Query && leq cod Theory = Just QueryTheory
  | leq dom Prim && leq cod Theory = Just PrimTheory
  | leq dom Theory = Just TheoryTop
  | otherwise = Nothing

class HasCodomain a b | a -> b where
  codOf :: a -> b

instance HasCodomain PiVariant Level where
  codOf = \case
    QueryTheory -> Theory
    PrimTheory -> Theory
    TheoryTop -> Top

data Abs a = Abs QName a | AbsConst a

data Fields a = Fields
  { names :: [QName],
    values :: [a]
  }
  deriving (Functor)

instance ElemAt (Fields a) QName a where
  elemAt (Fields xs vs) x = go xs vs
    where
      go (x' : xs') (v : vs')
        | x == x' = v
        | otherwise = go xs' vs'
      go _ _ = panic "`elemAt xs i` should only be called if i is a valid index into xs"

type data Energy = Kinetic | Potential

type K = Kinetic

type P = Potential

data SEnergy :: Energy -> Type where
  SKinetic :: SEnergy Kinetic
  SPotential :: SEnergy Potential

data Literal
  = LitInt Int
  | LitString Text
  deriving (Eq)

instance Pretty Literal where
  pretty = \case
    LitInt i -> pretty i
    LitString t -> "\"" <> pretty t <> "\""

data BuiltinTy
  = BuiltinInt
  | BuiltinString
  deriving (Eq)

instance Pretty BuiltinTy where
  pretty = \case
    BuiltinInt -> "Int"
    BuiltinString -> "String"

data ElS :: Energy -> Type where
  Var :: BId -> ElS K
  GlobalVar :: Constant -> ElS K
  Code :: (TyS e) -> ElS e
  Lam :: ~(TyS K) -> Abs (ElS e) -> ElS e
  App :: ElS e -> ElS K -> ElS e
  Cons :: Fields (ElS e) -> ElS e
  Proj :: ElS e -> QName -> ElS e
  Lit :: Literal -> ElS K

data TyS :: Energy -> Type where
  U :: Universe -> TyS e
  Decode :: Universe -> ElS e -> TyS e
  Pi :: PiVariant -> TyS K -> Abs (TyS K) -> TyS e
  Record :: Level -> [QName] -> [TyS K] -> TyS P
  BuiltinTy :: BuiltinTy -> TyS e

type Env = Bwd (ElV K)

data Clo a = Clo QName (ElV K -> a) | CloConst a

data Spine
  = SId
  | SApp Spine (ElV K)
  | SProj Spine QName

data Constant = Constant {name :: QName}
  deriving (Eq, Ord)

instance Pretty Constant where
  pretty (Constant x) = pretty x

data Head
  = Local FId
  | Global Constant
  deriving (Eq)

-- TODO: support lazy eta-expansion of potential pi-type neutrals
-- by splitting `canon` into `behavior` and `eta`
data Neutral = Neutral
  { head :: Head,
    spine :: Spine,
    behavesAs :: ~(Maybe (ElV P)),
    fields :: ~(Maybe (Fields (ElV K))),
    ty :: ~(TyV K)
  }

data ElV :: Energy -> Type where
  VNeu :: Neutral -> ElV K
  VCode :: TyV e -> ElV e
  VLam :: ~(TyV K) -> Clo (ElV e) -> ElV e
  VCons :: Fields (ElV e) -> ElV e
  VLit :: Literal -> ElV K

data TeleV a = TVNil | TVCons a (ElV K -> TeleV a)

data TyV :: Energy -> Type where
  VU :: Universe -> TyV e
  VDecode :: Universe -> Neutral -> TyV K
  VPi :: PiVariant -> TyV K -> Clo (TyV K) -> TyV e
  VRecord :: Level -> [QName] -> TeleV (TyV K) -> TyV P
  VBuiltinTy :: BuiltinTy -> TyV e

data GlobalEntry
  = KEntry (ElS K) (ElV K) (TyV K)
  | PEntry (ElS P) (ElV P) (TyV K)

newtype GlobalEnv = GlobalEnv (Map Constant GlobalEntry)

emptyGlobalEnv :: GlobalEnv
emptyGlobalEnv = GlobalEnv Map.empty

insertEntry :: GlobalEnv -> Constant -> GlobalEntry -> GlobalEnv
insertEntry (GlobalEnv m) c e = GlobalEnv (Map.insert c e m)

globalEntries :: GlobalEnv -> [(Constant, GlobalEntry)]
globalEntries (GlobalEnv m) = Map.toList m

instance ElemAt GlobalEnv Constant GlobalEntry where
  elemAt (GlobalEnv m) c = m Map.! c

instance Lookup GlobalEnv Constant GlobalEntry where
  lookup (GlobalEnv m) c = Map.lookup c m
