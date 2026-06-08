module Coln.Backend.TypeScript where

import Data.String (IsString (..))
import Data.Text (Text)
import Diagnostician (DDoc, DPretty (..))
import Coln.Common
import Coln.Core.Params
import Prettyprinter hiding (tupled)

-- Pretty printing utilities
--------------------------------------------------------------------------------

tupled :: [Doc a] -> Doc a
tupled =
  group
    . enclose ("(" <> line') (line' <> ")")
    . vsep
    . punctuate ", "
    . map (\d -> flatAlt (indent 2 d) d)

blocked :: [Doc a] -> Doc a
blocked =
  group
    . enclose ("{" <> line) (line <> "}")
    . vsep
    . map (\d -> flatAlt (indent 2 d) d)

-- Typescript AST
--------------------------------------------------------------------------------

class Assemble a where
  asm :: a -> DDoc

newtype Id = Id DDoc

instance IsString Id where
  fromString = Id . fromString

instance Assemble Id where
  asm (Id a) = a

data QId = QId [Id] Id

instance Assemble QId where
  asm (QId xs y) = mconcat [asm x <> "." | x <- xs] <> asm y

data RuntimeConst
  = Value
  | RelTable
  | FunTable
  | ReadonlySet
  | ReadWriteSet
  | IdGenerator
  deriving (Show)

class Runtime a where
  runtime :: RuntimeConst -> a

instance Runtime QId where
  runtime c = QId ["runtime"] (Id $ pretty $ show c)

instance Runtime Ty where
  runtime = TyConst . runtime

instance Runtime El where
  runtime = Const . runtime

data Ty
  = Fun Binding Ty
  | TyConst QId
  | ListTy Ty

instance Assemble Ty where
  asm (Fun bnd ret) = parens (asm bnd) <+> "=>" <+> asm ret
  asm (TyConst i) = asm i
  asm (ListTy a) = asm a <> "[]"

data Binding = Binding {name :: Id, ty :: Ty}

instance Assemble Binding where
  asm b = asm b.name <> ":" <+> asm b.ty

data BinOp = EqualsEquals

instance Assemble BinOp where
  asm EqualsEquals = "=="

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

instance Assemble El where
  asm (Var x) = asm x
  asm (Const x) = asm x
  asm (MethodCall t x args) =
    asm t <> "." <> asm x <> tupled (asm <$> args)
  asm (Lam bnd block) =
    parens (asm bnd) <+> "=>" <+> asm block
  asm (Lit (LitInt i)) = pretty i
  asm (Lit (LitString s)) = "\"" <> pretty s <> "\""
  asm (New x args) = parens ("new" <+> asm x <> tupled (asm <$> args))
  asm (Proj t x) = asm t <> "." <> asm x
  asm (String d) = "\"" <> d <> "\""
  asm (List ts) = list $ asm <$> ts
  asm (BinApp op t0 t1) = parens (asm t0 <+> asm op <+> asm t1)
  asm (Throw msg) = "throw" <+> "\"" <> msg <> "\""
  asm (Index t i) = asm t <> "[" <> pretty i <> "]"
  asm (Not t) = "!" <> asm t
  asm (Object fields) =
    blocked $ punctuate "," [asm x <> ":" <+> asm t | (x, t) <- fields]

data Statement
  = Let Id El
  | Assign QId El
  | Expr El
  | Cond (El, Block) [(El, Block)] (Maybe Block)

instance Assemble Statement where
  asm (Let x t) = "const" <+> asm x <+> "=" <+> asm t <> ";"
  asm (Assign x t) = asm x <+> "=" <+> asm t <> ";"
  asm (Expr t) = asm t <> ";"
  asm (Cond (t0, b0) elseIfs elseBlock) =
    "if"
      <+> parens (asm t0)
      <+> asm b0
      <+> cat ["else if" <+> parens (asm t) <+> asm b | (t, b) <- elseIfs]
      <+> maybe mempty (\b -> "else" <+> asm b) elseBlock

data Block = Block {statements :: [Statement], return :: Maybe El}

instance Assemble Block where
  asm b =
    let ret = case b.return of
          Just t -> ["return" <+> asm t <> ";"]
          Nothing -> []
     in blocked $ (asm <$> b.statements) ++ ret

data Class = Class
  { name :: Id
  , implements :: Id
  , fields :: [Binding]
  , constructor :: Block
  }

instance Assemble Class where
  asm c =
    "class"
      <+> asm c.name
      <+> "implements"
      <+> asm c.implements
      <+> blocked
        ( punctuate
            line
            [ vsep [asm f <> ";" | f <- c.fields]
            , "constructor()" <+> asm c.constructor
            ]
        )

data Interface = Interface
  { name :: Id
  , extends :: Maybe Id
  , fields :: [Binding]
  }

instance Assemble Interface where
  asm i =
    "interface"
      <+> asm i.name
      <+> maybe mempty (\e -> "extends" <+> asm e <> " ") i.extends
      <> blocked [asm f <> ";" | f <- i.fields]

data Function = Function
  { name :: Id
  , args :: [Binding]
  , ret :: Maybe Ty
  , body :: Block
  }

instance Assemble Function where
  asm f =
    "function"
      <+> asm f.name
      <> tupled (asm <$> f.args)
      <> maybe mempty (\ty -> ":" <+> asm ty) f.ret
      <+> asm f.body

data AccessControlled a
  = Exported a
  | Private a

instance (Assemble a) => Assemble (AccessControlled a) where
  asm (Exported a) = "export" <+> asm a
  asm (Private a) = asm a

data Namespace = Namespace
  { name :: Id
  , members :: [AccessControlled Declaration]
  }

instance Assemble Namespace where
  asm ns =
    vsep
      [ "namespace" <+> asm ns.name <+> "{"
      , indent 2 $ vsep $ punctuate line $ asm <$> ns.members
      , "}"
      ]

data Declaration
  = DFunction Function
  | DClass Class
  | DInterface Interface
  | DNamespace Namespace

instance Assemble Declaration where
  asm = \case
    DFunction f -> asm f
    DClass c -> asm c
    DInterface i -> asm i
    DNamespace n -> asm n

data Module = Module
  { imports :: [Text]
  , declarations :: [AccessControlled Declaration]
  }

instance Assemble Module where
  asm m =
    vsep
      [ vsep $ pretty <$> m.imports
      , ""
      , vsep $ punctuate line $ asm <$> m.declarations
      ]

