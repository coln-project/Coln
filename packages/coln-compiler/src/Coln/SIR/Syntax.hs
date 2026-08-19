module Coln.SIR.Syntax where

import Coln.Common
import Coln.Core.Params

data El :: MLevel -> Type where
  LiftEl :: El Set -> El Theory
  Var :: BId -> El Set
  Single :: Query -> El Set
  Proj :: El Set -> Name -> El Set
  Holds :: Pred -> El Theory
  Multi :: Query -> El Theory
  Lam :: Abs (El Theory) -> El Theory
  Cons :: Dict (El l) -> El l
  Lit :: Literal -> El Set

data Pred
  = Atom TableName (Maybe (El Set)) [El Set]
  | And (Dict Pred)
  | Eq Shape (El Set) (El Set)

data Shape
  = RowId TableName
  | Tuple (Dict Shape)
  | BuiltinTy BuiltinTy

unitShape :: Shape
unitShape = Tuple (fromList [])

truePred :: Pred
truePred = And (fromList [])

data Abs a
  = Abs (Maybe Name) a
  | AbsConst a

data Query = Query
  { shape :: Shape
  , pred :: Abs Pred
  }

