module Coln.SIR.Syntax where

import Coln.Common
import Coln.Core.Params
import Coln.MIR.Params

import Data.Aeson qualified as AE
import Data.Aeson.Encoding qualified as AE
import Data.Char (toLower)
import GHC.Generics

data PrimQuery
  = PrimaryKey TableName [El Set]

data El :: MLevel -> Type where
  LiftEl :: El Set -> El Theory
  Var :: BId -> El Set
  Lookup :: TableName -> [El Set] -> Shape -> El Set
  Proj :: El Set -> Name -> El Set
  SelectRowId :: SUniverse Set Theory -> TableName -> [El Set] -> El Theory
  SelectLast :: SUniverse Set Theory -> TableName -> [El Set] -> Shape -> El Theory
  Lam :: Query -> Abs (El Theory) -> El Theory
  Cons :: Dict (El l) -> El l
  Lit :: Literal -> El Set
  Erased :: El Set

data Prop
  = Atom TableName (El Set) [El Set]
  | And (Dict Prop)
  | Eq Shape (El Set) (El Set)

data ScalarType
  = RowId TableName
  | BuiltinTy BuiltinTy
  deriving (Eq, Show, Generic)

data Shape
  = Tuple (Dict Shape)
  | Scalar ScalarType
  | Unstored

shapeSize :: Shape -> Int
shapeSize = \case
  Tuple ds -> sum $ shapeSize <$> toList ds.values
  Scalar _ -> 1
  Unstored -> 0

trueProp :: Prop
trueProp = And (fromList [])

data Abs a
  = Abs (Maybe Name) a
  | AbsConst a

data Query = Query
  { shape :: Shape
  , pred :: Abs Prop
  }

-- JSON
--------------------------------------------------------------------------------

aeOptions :: AE.Options
aeOptions =
  AE.defaultOptions
    { AE.allNullaryToStringTag = True
    , AE.constructorTagModifier = \x -> fmap toLower (take 1 x) ++ (drop 1 x)
    }

class PathLike a where
  namesOf :: a -> [Name]

encName :: Name -> AE.Encoding
encName n = AE.list AE.toEncoding $ n.init ++ [n.last]

encPath :: (PathLike a) => a -> AE.Encoding
encPath = AE.list encName . namesOf

instance PathLike Path where namesOf = toList

instance PathLike TableName where namesOf tn = tn.realm : namesOf tn.path

taggedEncoding :: Text -> AE.Series -> AE.Encoding
taggedEncoding t v = AE.pairs $ AE.pair "tag" (AE.toEncoding t) <> v

instance AE.ToJSON ScalarType where
  toJSON = panic "aesons behaving badly"
  toEncoding = \case
    RowId e -> taggedEncoding "rowId" $ AE.pair "path" $ encPath e
    BuiltinTy bt -> taggedEncoding "builtin" $ AE.pair "type" $ AE.genericToEncoding aeOptions bt
