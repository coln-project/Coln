module Coln.MIR.Syntax where

import Coln.Common
import Coln.MIR.Params
import Coln.Core.Params

data Abs a = Abs Name a | AbsConst a

data El :: MLevel -> Type where
  LiftEl :: El Set -> El Theory
  Var :: BId -> El Set
  Lookup :: TableName -> [El Set] -> Ty Set -> El Set
  Code :: SUniverse Set Theory -> Ty Set -> El Theory
  Lam :: Abs (El Theory) -> El Theory
  Cons :: Dict (El l) -> El l
  Proj :: El l -> Name -> El l
  Lit :: Literal -> El Set


data FunctionType = FunctionType
  { dom :: Ty Set
  , cod :: Abs (Ty Theory)
  }

data RecordType l = RecordType
  { fieldTypes :: Dict (Ty l) }

data Ty :: MLevel -> Type where
  LiftTy :: Ty Set -> Ty Theory
  U :: SUniverse Set Theory -> Ty Theory
  EltOf :: TableName -> [El Set] -> Ty Set
  Function :: FunctionType -> Ty Theory
  Record :: RecordType l -> Ty l
  BuiltinTy :: BuiltinTy -> Ty Set
  Eq :: Ty Set -> El Set -> El Set -> Ty Set
  
