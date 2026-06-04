module Coln.Core where

import Data.Kind (Type)
import Data.Map.Strict (Map)
import Data.Map.Strict qualified as Map
import Data.Text (Text)
import Diagnostician
import FNotation (Name)
import Coln.Common
import Prettyprinter

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

universeFor :: Level -> Maybe Universe
universeFor = \case
  Set -> Just SetU
  Theory -> Just TheoryU
  Top -> Nothing

data BindingMode = BInductive | BConjunctive
  deriving (Eq, Show)

-- NOTE: we could potentially replace TheoryTop with TopTop, which would allow
-- for higher-order stuff. Not sure whether this is semantically good though? Or
-- whether this would mess with things we care about...
data PiVariant
  = SetTheory BindingMode
  | TheoryTop BindingMode
  deriving (Eq, Show)

instance Pretty PiVariant where
  pretty = pretty . show

instance LevelOf PiVariant where
  levelOf = \case
    SetTheory _ -> Theory
    TheoryTop _ -> Theory

bindingMode :: PiVariant -> BindingMode
bindingMode (SetTheory bm) = bm
bindingMode (TheoryTop bm) = bm

piVariant :: Level -> Level -> BindingMode -> Maybe PiVariant
piVariant dom cod bm
  | leq dom Set && leq cod Theory = Just $ SetTheory bm
  | leq dom Theory = Just $ TheoryTop bm
  | otherwise = Nothing

class HasCodomain a b | a -> b where
  codOf :: a -> b

instance HasCodomain PiVariant Level where
  codOf = \case
    SetTheory _ -> Theory
    TheoryTop _ -> Top

data Abs a = Abs Name a | AbsConst a

data Fields a = Fields
  { names :: [Name]
  , values :: [a]
  }
  deriving (Functor)

instance ElemAt (Fields a) Name a where
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

instance Show BuiltinTy where
  show = \case
    BuiltinInt -> "Int"
    BuiltinString -> "String"

data ElS :: Energy -> Type where
  LocalVar :: BId -> ElS K
  GlobalVar :: Name -> ElS K
  Code :: (TyS e) -> ElS e
  Lam :: ~(TyS K) -> Abs (ElS e) -> ElS e
  App :: ElS e -> ElS K -> ElS e
  Cons :: Fields (ElS e) -> ElS e
  Proj :: ElS e -> Name -> ElS e
  Lit :: Literal -> ElS K
  Init :: TyS K -> ElS K
  Pure :: ElS K -> ElS K
  Use :: ElS K -> ElS K

data TeleS e = TSNil | TSCons (TyS e) (TeleS e)

data TyS :: Energy -> Type where
  U :: Universe -> TyS e
  Decode :: ElS e -> TyS e
  Pi :: PiVariant -> TyS K -> Abs (TyS K) -> TyS e
  Record :: Level -> [Name] -> TeleS K -> TyS P
  Eq :: TyS K -> ElS K -> ElS K -> TyS K
  BuiltinTy :: BuiltinTy -> TyS e
  Inductive :: TyS K -> TyS K

type Env = Bwd (ElV K)

data Clo a = Clo Name (ElV K -> a) | CloConst a

data Spine
  = SId
  | SApp Spine (ElV K)
  | SProj Spine Name
  | SUse Spine

data Head
  = VLocal FId
  | VGlobal Name
  | VInit (TyV K)

-- TODO: support lazy eta-expansion of potential pi-type neutrals
-- by splitting `canon` into `behavior` and `eta`
data Neutral = Neutral
  { head :: Head
  , spine :: Spine
  , behavesAs :: ~(Maybe (ElV P))
  , fields :: ~(Maybe (Fields (ElV K)))
  , ty :: ~(TyV K)
  }

data ElV :: Energy -> Type where
  VNeu :: Neutral -> ElV K
  VCode :: TyV e -> ElV e
  VLam :: ~(TyV K) -> Clo (ElV e) -> ElV e
  VCons :: Fields (ElV e) -> ElV e
  VLit :: Literal -> ElV K
  VPure :: ElV K -> ElV K

data TeleV e = TVNil | TVCons (TyV e) (ElV K -> TeleV e)

data TyV :: Energy -> Type where
  VU :: Universe -> TyV e
  VDecode :: Neutral -> TyV K
  VPi :: PiVariant -> TyV K -> Clo (TyV K) -> TyV e
  VRecord :: Level -> [Name] -> TeleV K -> TyV P
  VEq :: TyV K -> ElV K -> ElV K -> TyV K
  VBuiltinTy :: BuiltinTy -> TyV e
  VInductive :: TyV K -> TyV K

instance LevelOf (TyV e) where
  levelOf = \case
    VU u -> codesInto u
    VDecode n -> case n.ty of
      VU u -> decodesInto u
      _ -> error "decoded a non-type"
    VPi pv _ _ -> codOf pv
    VRecord l _ _ -> l
    VEq _ _ _ -> Set
    VBuiltinTy _ -> Set
    VInductive a -> levelOf a

data GlobalEntry
  = KEntry (ElS K) (ElV K) (TyV K)
  | PEntry (ElS P) (ElV P) (TyV K)

newtype GlobalEnv = GlobalEnv (Map Name GlobalEntry)

emptyGlobalEnv :: GlobalEnv
emptyGlobalEnv = GlobalEnv Map.empty

insertEntry :: GlobalEnv -> Name -> GlobalEntry -> GlobalEnv
insertEntry (GlobalEnv m) c e = GlobalEnv (Map.insert c e m)

globalEntries :: GlobalEnv -> [(Name, GlobalEntry)]
globalEntries (GlobalEnv m) = Map.toList m

instance ElemAt GlobalEnv Name GlobalEntry where
  elemAt (GlobalEnv m) c = m Map.! c

instance Lookup GlobalEnv Name GlobalEntry where
  lookup (GlobalEnv m) c = Map.lookup c m
