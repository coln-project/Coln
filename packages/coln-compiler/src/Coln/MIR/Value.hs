module Coln.MIR.Value where

-- MIR consists of models in a context which only has set-level variables

import Coln.Common
import Coln.Core.Params

type Locals = Bwd Model

data Clo a
  = Clo Name (Model -> a)
  | CloConst a

data Shape
  = RowId TableName
  | Tuple (Dict Shape)
  | BuiltinTy BuiltinTy

data Neutral = Neutral
  { head :: FId
  , spine :: Bwd Name -- only projections
  }

data El
  = Neu Neutral
  | SetCons (Dict El)
  | Lit Literal
  | Single Ty

data Pred
  = EltOf TableName (Maybe El) [Maybe El]
  | And (Dict Pred)

data Ty = Ty
  { shape :: Shape
  , pred :: El -> Pred
  }

data Model
  = All Ty
  | Lift El
  | Lam (Clo Model)
  | ModelCons (Dict Model)

data FunctionType = FunctionType
  { dom :: Ty
  , cod :: El -> Theory
  }

data RecordType = RecordType
  { capture :: Locals
  , fieldTypes :: Dict (Locals -> Theory)
  }

data Theory
  = SetU
  | PropU
  | Elt Ty
  | Function FunctionType
  | Record RecordType

data Top
  = Model Model
  | TheoryCode Theory
  | TopLam (Clo Top)
