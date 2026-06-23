-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Backend.TypeScript.AST where

import Data.String (IsString (..))
import Data.Text (Text)
import Data.Text.Lazy qualified as TL
import Diagnostician (DDoc, DPretty (..))
import Prettyprinter
import Prettyprinter.Render.Text

import Coln.Backend.TypeScript.Params
import Coln.Common
import Coln.Core.Params

newtype Id = Id {content :: DDoc}

instance IsString Id where
  fromString = Id . fromString

instance DPretty Id where
  dpretty x = x.content

idToString :: Id -> String
idToString (Id x) = TL.unpack $ renderLazy $ layoutPretty defaultLayoutOptions x

data QId = QId [Id] Id

instance IsString QId where
  fromString = QId [] . fromString

class Runtime a where
  runtime :: RuntimeConst -> a

instance Runtime QId where
  runtime = \case
    ColnSet access -> QId ["runtime", "ColnSet"] (fromShow access)
    RowIdSet access -> QId ["runtime", "RowIdSet"] (fromShow access)
    ColnRef access -> QId ["runtime", "ColnRef"] (fromShow access)
    TableCellRef access -> QId ["runtime", "TableCellRef"] (fromShow access)
    x -> QId ["runtime"] (fromShow x)

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
  | Call El [El]
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
  , constructor :: Constructor
  }

data Constructor = Constructor
  { args :: [Binding]
  , body :: Block
  }

data Interface = Interface
  { name :: Id
  , extends :: Maybe Id
  , fields :: [Binding]
  }

data FunctionDef = FunctionDef
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
  = DFunctionDef FunctionDef
  | DClass Class
  | DInterface Interface
  | DTypeDef TypeDef

data Import
  = ImportQualified Id DDoc
  | ImportSpecific Id DDoc
  | ImportSpecificExported Id DDoc

data Module = Module
  { imports :: [Import]
  , declarations :: [AccessControlled Declaration]
  }
