{-# LANGUAGE TypeAbstractions #-}
module Coln.MIR.Value where

import Coln.Common
import Coln.Core.Params
import Coln.MIR.Params

-- MIR consists of models in a context which only has set-level variables

data Head
  = Var FId
  | Lookup TableName [El N Set] (Ty N Set)

data Neutral = Neutral
  { head :: Head
  , spine :: Bwd Name
  }

type Locals = Bwd (Match SMLevel (El N))

type Globals = OMap Name (Match SMLevel (El N))

data Clo a b = Clo Name (a -> b) | CloConst b

appClo :: Clo a b -> a -> b
appClo (Clo _ f) v = f v
appClo (CloConst v) _ = v

data El :: Case -> MLevel -> Type where
  LiftEl :: Lift l0 l1 -> El c l0 -> El c l1
  Neu :: Neutral -> El N Set
  Init :: Ty N Theory -> El D Theory
  Code :: SUniverse l0 l1 -> Ty N l0 -> El N l1
  Lam :: SMFunctionVariant l0 l1 -> Ty N l0 -> Clo (El N l0) (Evaluation El c l1) -> El c l1
  Cons :: Dict (Evaluation El c l) -> El c l
  Lit :: Literal -> El N Set
  Erased :: El N Set

local :: FId -> El N Set
local i = Neu $ Neutral (Var i) BwdNil

lookup :: TableName -> [El N Set] -> Ty N Set -> El N Set
lookup tn args a = Neu $ Neutral (Lookup tn args a) BwdNil

app :: SMFunctionVariant l0 l1 -> El N l1 -> El N l0 -> El N l1
app fv (Lam fv' _ clo) v = case (fv, fv') of
  (SSetTheory, SSetTheory) -> appClo clo v
  (STheoryTop, STheoryTop) -> appClo clo v
app _ _ _ = panic "can only apply lambda"

proj :: El N l -> Name -> El N l
proj (Neu n) x = Neu $ n{spine = n.spine :> x}
proj (Cons fields) x = elemAt fields x
proj Erased _ = Erased
proj _ _ = panic "can only project from neutral or cons"

decode :: SUniverse l0 l1 -> El N l1 -> Ty N l0
decode su (Code su' a) = case (su, su') of
  (SPropU, SPropU) -> a
  (SSetU, SPropU) -> a
  (SSetU, SSetU) -> a
  (SPropU, SSetU) -> panic "tried to decode a set into a proposition"
  (STheoryU, STheoryU) -> a
decode _ _ = panic "tried to decode a non-code"

instance LevelCoerce (El c) where
  levelCoerce SSet SSet v = v
  levelCoerce STheory STheory v = v
  levelCoerce STop STop v = v
  levelCoerce SSet STheory v = LiftEl LSetTheory v
  levelCoerce STheory STop v = LiftEl LTheoryTop v
  levelCoerce SSet STop v = LiftEl LTheoryTop (LiftEl LSetTheory v)
  levelCoerce STheory SSet (LiftEl LSetTheory v) = v
  levelCoerce STop STheory (LiftEl LTheoryTop v) = v
  levelCoerce STop SSet (LiftEl LTheoryTop (LiftEl LSetTheory v)) = v
  levelCoerce _ _ _ = panic "cannot level coerce"

data FunctionType (l0 :: MLevel) (l1 :: MLevel) = FunctionType
  { variant :: SFunctionVariant l0 l1
  , dom :: Ty N l0
  , cod :: Clo (El N l0) (Ty N l1)
  }

data RecordType (l :: MLevel) = RecordType
  { hlevel :: HLevel
  , capture :: Locals
  , fieldTypes :: Dict (Locals -> Ty N l)
  }

data Ty :: Case -> MLevel -> Type where
  LiftTy :: Lift l0 l1 -> Ty c l0 -> Ty c l1
  U :: SUniverse l0 l1 -> Ty N l1
  EltOf :: SUniverse Set Theory -> TableName -> [El N Set] -> Ty N Set
  Function :: FunctionType l0 l1 -> Ty N l1
  Record :: RecordType l -> Ty N l
  BuiltinTy :: BuiltinTy -> Ty N Set
  Eq :: Ty N Set -> El N Set -> El N Set -> Ty N Set

instance LevelCoerce (Ty c) where
  levelCoerce SSet SSet v = v
  levelCoerce STheory STheory v = v
  levelCoerce STop STop v = v
  levelCoerce SSet STheory v = LiftTy LSetTheory v
  levelCoerce STheory STop v = LiftTy LTheoryTop v
  levelCoerce SSet STop v = LiftTy LTheoryTop (LiftTy LSetTheory v)
  levelCoerce STheory SSet (LiftTy LSetTheory v) = v
  levelCoerce STop STheory (LiftTy LTheoryTop v) = v
  levelCoerce STop SSet (LiftTy LTheoryTop (LiftTy LSetTheory v)) = v
  levelCoerce _ _ _ = panic "cannot lift"

instance HLevelOf (Ty c Set) where
  hlevelOf = \case
    EltOf SPropU _ _ -> HProp
    EltOf SSetU _ _ -> HSet
    Record rt -> rt.hlevel
    Eq eat _ _ -> equalityHLevelOf (hlevelOf eat)
    BuiltinTy _ -> HSet

type family Evaluation (f :: Case -> MLevel -> Type) (c :: Case) = (r :: MLevel -> Type) | r -> c f where
  Evaluation f N = f N
  Evaluation f D = Description f

data Description :: (Case -> MLevel -> Type) -> MLevel -> Type where
  Describe :: f D l -> Description f l
  Become :: f N l -> Description f l

class HasEvaluation (c :: Case) where
  epure :: a c l -> Evaluation a c l
  emap :: (forall c'. (HasEvaluation c') => a c' l0 -> b c' l1) -> Evaluation a c l0 -> Evaluation b c l1
  ebind :: (forall c'. (HasEvaluation c') => a c' l0 -> Evaluation b c' l1) -> Evaluation a c l0 -> Evaluation b c l1
  scase :: SCase c

instance HasEvaluation N where
  epure = id
  emap f = f
  ebind f = f
  scase = SNominative

instance HasEvaluation D where
  epure = Describe
  emap f (Describe x) = Describe (f x)
  emap f (Become x) = Become (f x)
  ebind f (Describe x) = f x
  ebind f (Become x) = Become (f x)
  scase = SDescriptive
