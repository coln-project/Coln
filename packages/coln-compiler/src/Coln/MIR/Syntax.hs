module Coln.MIR.Syntax where

import Coln.Common
import Coln.Core.Params
import Coln.MIR.Params

data Abs a = Abs Name a | AbsConst a

data El :: Case -> MLevel -> Type where
  LiftEl :: El c Set -> El c Theory
  Var :: BId -> El N Set
  Lookup :: TableName -> [El N Set] -> Ty N Set -> El N Set
  Code :: SUniverse Set Theory -> Ty N Set -> El N Theory
  Lam :: Ty N Set -> Abs (El c Theory) -> El c Theory
  Cons :: Dict (El c l) -> El c l
  Proj :: El N l -> Name -> El N l
  Lit :: Literal -> El N Set
  Is :: El N l -> El D l
  Erased :: El N Set

data FunctionType = FunctionType
  { variant :: SFunctionVariant Set Theory
  , dom :: Ty N Set
  , cod :: Abs (Ty N Theory)
  }

data RecordType l = RecordType
  { hlevel :: HLevel
  , fieldTypes :: Dict (Ty N l)
  }

data Ty :: Case -> MLevel -> Type where
  LiftTy :: Ty c Set -> Ty c Theory
  U :: SUniverse Set Theory -> Ty N Theory
  EltOf :: SUniverse Set Theory -> TableName -> [El N Set] -> Ty N Set
  Function :: FunctionType -> Ty N Theory
  Record :: RecordType l -> Ty N l
  BuiltinTy :: BuiltinTy -> Ty N Set
  Eq :: Ty N Set -> El N Set -> El N Set -> Ty N Set
