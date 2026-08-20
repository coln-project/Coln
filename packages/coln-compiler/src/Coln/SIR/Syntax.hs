module Coln.SIR.Syntax where

import Coln.Common
import Coln.Core.Params
import Coln.MIR.Params

import GHC.Generics

data El :: MLevel -> Type where
  LiftEl :: El Set -> El Theory
  Var :: BId -> El Set
  Single :: Query -> El Set
  Proj :: El Set -> Name -> El Set
  Multi :: SUniverse Set Theory -> Query -> El Theory
  Lam :: Query -> Abs (El Theory) -> El Theory
  Cons :: Dict (El l) -> El l
  Lit :: Literal -> El Set

data Prop
  = Atom TableName (Maybe (El Set)) [El Set]
  | And (Dict Prop)
  | Eq Shape (El Set) (El Set)

data ScalarType
  = RowId TableName
  | BuiltinTy BuiltinTy
  deriving (Eq, Show, Generic)

data Shape
  = Tuple (Dict Shape)
  | Scalar ScalarType

shapeSize :: Shape -> Int
shapeSize = \case
  Tuple ds -> sum $ shapeSize <$> toList ds.values
  Scalar _ -> 1

unitShape :: Shape
unitShape = Tuple (fromList [])

trueProp :: Prop
trueProp = And (fromList [])

data Abs a
  = Abs (Maybe Name) a
  | AbsConst a

data Query = Query
  { shape :: Shape
  , pred :: Abs Prop
  }
