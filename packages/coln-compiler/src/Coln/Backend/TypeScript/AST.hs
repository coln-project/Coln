module Coln.Backend.TypeScript.AST where

import Data.String (IsString (..))
import Data.Text (Text)
import Data.Text.Lazy qualified as TL
import Diagnostician (DDoc, DPretty (..))
import Prettyprinter
import Prettyprinter.Render.Text

import Coln.Common
import Coln.Core.Params
import Coln.Backend.TypeScript.Params

newtype Id = Id DDoc

instance IsString Id where
  fromString = Id . fromString

idToString :: Id -> String
idToString (Id x) = TL.unpack $ renderLazy $ layoutPretty defaultLayoutOptions x

data QId = QId [Id] Id

instance IsString QId where
  fromString = QId [] . fromString

class Runtime a where
  runtime :: RuntimeConst -> a

instance Runtime QId where
  runtime = \case
    Value -> QId ["runtime"] "Value"
    ColnSet access -> QId ["runtime", "ColnSet"] (fromString (show access))

instance Runtime Ty where
  runtime = TyConst . runtime

instance Runtime El where
  runtime = Const . runtime

data Ty
  = Fun Binding Ty
  | TyConst QId
  | ListTy Ty

data Binding = Binding {name :: Id, ty :: Ty}

data BinOp = EqualsEquals

data El
  = Var Id
  | Const QId
  | MethodCall El Id [El]
  | Lam Binding Block
  | Lit Literal
  | New El [El]
  | Proj El Id
  | String DDoc
  | List [El]
  | BinApp BinOp El El
  | Throw DDoc
  | Index El Int
  | Not El
  | Object [(Id, El)]

data Statement
  = Let Id El
  | Assign QId El
  | Expr El
  | Cond (El, Block) [(El, Block)] (Maybe Block)


data Block = Block {statements :: [Statement], return :: Maybe El}

data Class = Class
  { name :: Id
  , implements :: Maybe QId
  , extends :: Maybe QId
  , fields :: [Binding]
  , constructor :: Block
  }

data Interface = Interface
  { name :: Id
  , extends :: Maybe Id
  , fields :: [Binding]
  }

data Function = Function
  { name :: Id
  , args :: [Binding]
  , ret :: Maybe Ty
  , body :: Block
  }

data TypeDef = TypeDef
  { name :: Id
  , body :: Ty
  }

data AccessControlled a
  = Exported a
  | Private a

data Declaration
  = DFunction Function
  | DClass Class
  | DInterface Interface
  | DTypeDef TypeDef

data Module = Module
  { imports :: [Text]
  , declarations :: [AccessControlled Declaration]
  }
