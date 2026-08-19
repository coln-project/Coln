module Coln.MIR.Value where

import Coln.Common
import Coln.Core.Params
import Coln.MIR.Params

-- MIR consists of models in a context which only has set-level variables

data Head
  = Var FId
  | Lookup TableName [El Set] (Ty Set)

data Neutral = Neutral
  { head :: Head
  , spine :: Bwd Name
  }

type Locals = Bwd (Match SMLevel El)

type Globals = OMap Name (Match SMLevel El)

data Clo a b = Clo Name (a -> b) | CloConst b

appClo :: Clo a b -> a -> b
appClo (Clo _ f) v = f v
appClo (CloConst v) _ = v

data El :: MLevel -> Type where
  LiftEl :: Lift l0 l1 -> El l0 -> El l1
  Neu :: Neutral -> El Set
  Code :: SUniverse l0 l1 -> Ty l0 -> El l1
  Lam :: SFunctionVariant l0 l1 -> Clo (El l0) (El l1) -> El l1
  Cons :: Dict (El l) -> El l
  Lit :: Literal -> El Set

local :: FId -> El Set
local i = Neu $ Neutral (Var i) BwdNil

lookup :: TableName -> [El Set] -> Ty Set -> El Set
lookup tn args a = Neu $ Neutral (Lookup tn args a) BwdNil

app :: SFunctionVariant l0 l1 -> El l1 -> El l0 -> El l1
app fv (Lam fv' clo) v = case (fv, fv') of
  (SSetTheory, SSetTheory) -> appClo clo v
  (STheoryTop, STheoryTop) -> appClo clo v
app _ _ _ = panic "can only apply lambda"

proj :: El l -> Name -> El l
proj (Neu n) x = Neu $ n { spine = n.spine :> x }
proj (Cons fields) x = elemAt fields x
proj _ _ = panic "can only project from neutral or cons"

decode :: SUniverse l0 l1 -> El l1 -> Ty l0
decode su (Code su' a) = case (su, su') of
  (SPropU, SPropU) -> a
  (SSetU, SPropU) -> a
  (SSetU, SSetU) -> a
  (SPropU, SSetU) -> panic "tried to decode a set into a proposition"
  (STheoryU, STheoryU) -> a
decode _ _ = panic "tried to decode a non-code"

instance LevelCoerce El where
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
  , dom :: Ty l0
  , cod :: Clo (El l0) (Ty l1)
  }

data RecordType (l :: MLevel) = RecordType
  { capture :: Locals
  , fieldTypes :: Dict (Locals -> Ty l)
  }

data Ty :: MLevel -> Type where
  LiftTy :: Lift l0 l1 -> Ty l0 -> Ty l1
  U :: SUniverse l0 l1 -> Ty l1
  EltOf :: TableName -> [El Set] -> Ty Set
  Function :: FunctionType l0 l1 -> Ty l1
  Record :: RecordType l -> Ty l
  BuiltinTy :: BuiltinTy -> Ty Set
  Eq :: Ty Set -> El Set -> El Set -> Ty Set
  
instance LevelCoerce Ty where
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
