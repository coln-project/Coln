-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Backend.TypeScript.Assemble where

import Data.String (IsString (..))
import Data.Text (Text)
import Diagnostician (DDoc, DPretty (..))
import Prettyprinter hiding (tupled)

import Coln.Backend.TypeScript.AST
import Coln.Common
import Coln.Core.Params

tupled :: [Doc a] -> Doc a
tupled =
  group
    . enclose ("(" <> line') (line' <> ")")
    . vsep
    . punctuate ","
    . map (\d -> flatAlt (indent 2 d) d)

blocked :: [Doc a] -> Doc a
blocked [] = "{}"
blocked ds =
  group $
    enclose ("{" <> line) (line <> "}") $
      vsep [flatAlt (indent 2 d) d | d <- ds]

hardBlocked :: [Doc a] -> Doc a
hardBlocked [] = "{" <> hardline <> "}"
hardBlocked ds =
  enclose ("{" <> hardline) (hardline <> "}") $
    vsep [indent 2 d | d <- ds]

class Assemble a where
  asm :: a -> DDoc

instance Assemble Id where
  asm (Id a) = a

instance Assemble QId where
  asm (QId xs y) = mconcat [asm x <> "." | x <- xs] <> asm y

instance Assemble Ty where
  asm (Fun bnd ret) = parens (asm bnd) <+> "=>" <+> asm ret
  asm (TyConst i) = asm i
  asm (ListTy a) = asm a <> "[]"

instance Assemble Binding where
  asm b = asm b.name <> ":" <+> asm b.ty

instance Assemble BinOp where
  asm EqualsEquals = "=="

instance Assemble El where
  asm (Var x) = asm x
  asm (Const x) = asm x
  asm (MethodCall t x args) =
    asm t <> "." <> asm x <> tupled (asm <$> args)
  asm (Call t args) = asm t <> tupled (asm <$> args)
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

instance Assemble Statement where
  asm (Let x t) = "const" <+> asm x <+> "=" <+> asm t <> ";"
  asm (Assign x t) = asm x <+> "=" <+> asm t <> ";"
  asm (Expr t) = asm t <> ";"
  asm (Cond (t, b0) elseIfs elseBlock) =
    "if"
      <+> parens (asm t)
      <+> asm b0
      <+> cat ["else if" <+> parens (asm t) <+> asm b | (t, b) <- elseIfs]
      <+> maybe mempty (\b -> "else" <+> asm b) elseBlock

instance Assemble Block where
  asm b =
    let ret = case b.return of
          Just t -> ["return" <+> asm t <> ";"]
          Nothing -> []
     in hardBlocked $ (asm <$> b.statements) ++ ret

instance Assemble Class where
  asm c =
    "class"
      <+> asm c.name
      <> maybe mempty (\e -> " extends" <+> asm e) c.extends
      <> maybe mempty (\i -> " implements" <+> asm i) c.implements
      <+> hardBlocked
        ( punctuate
            line
            [ vsep [asm f <> ";" | f <- c.fields]
            , asm c.constructor
            ]
        )

instance Assemble Constructor where
  asm c =
    "constructor"
      <> tupled (asm <$> c.args)
      <+> asm c.body

instance Assemble Interface where
  asm i =
    "interface"
      <+> asm i.name
      <+> maybe mempty (\e -> "extends" <+> asm e <> " ") i.extends
      <> hardBlocked [asm f <> ";" | f <- i.fields]

instance Assemble FunctionDef where
  asm f =
    "function"
      <+> asm f.name
      <> tupled (asm <$> f.args)
      <> maybe mempty (\ty -> ":" <+> asm ty) f.ret
      <+> asm f.body

instance Assemble TypeDef where
  asm td =
    "type"
      <+> asm td.name
      <+> "="
      <+> asm td.body
      <> ";"

instance (Assemble a) => Assemble (AccessControlled a) where
  asm (Exported a) = "export" <+> asm a
  asm (Private a) = asm a

instance Assemble Declaration where
  asm = \case
    DFunctionDef f -> asm f
    DClass c -> asm c
    DInterface i -> asm i
    DTypeDef td -> asm td

instance Assemble Import where
  asm = \case
    ImportQualified x from ->
      "import * as" <+> asm x <+> "from" <+> surround from "\"" "\"" <> ";"
    ImportSpecific x from ->
      "import" <+> asm x <+> "from" <+> surround from "\"" "\"" <> ";"
    ImportSpecificExported x from ->
      vsep
        [ "import" <+> asm x <+> "from" <+> surround from "\"" "\"" <> ";"
        , "export" <+> braces (asm x) <> ";"
        ]

instance Assemble Module where
  asm m =
    vsep
      [ vsep $ asm <$> m.imports
      , ""
      , vsep $ punctuate line $ asm <$> m.declarations
      ]
