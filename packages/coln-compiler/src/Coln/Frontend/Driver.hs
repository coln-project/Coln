-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT
{-# LANGUAGE TypeAbstractions #-}

module Coln.Frontend.Driver where

import Control.Exception (try)
import Data.Foldable
import Data.Functor.Contravariant (contramap)
import Data.List.NonEmpty (NonEmpty (..))
import Diagnostician
import FNotation (Ntn)
import FNotation qualified as N

import Coln.Common
import Coln.Core.Globals
import Coln.Core.Layout
import Coln.Core.Memoed qualified as M
import Coln.Core.Params
import Coln.Core.Readback
import Coln.Core.Realm
import Coln.Core.Syntax qualified as S
import Coln.Core.Value qualified as V
import Coln.Diagnostics
import Coln.Elaborator.Debug
import Coln.Elaborator.Environment
import Coln.Elaborator.Judgment
import Coln.Elaborator.Rules.Builtin qualified as Builtin
import Coln.Elaborator.Rules.Equality qualified as Equality
import Coln.Elaborator.Rules.Function qualified as Function
import Coln.Elaborator.Rules.Record qualified as Record
import Coln.Elaborator.Rules.Universe qualified as Universe
import Coln.Elaborator.Rules.Variable qualified as Variable
import Coln.Frontend.Diagnostics
import Coln.Frontend.Notation
import Coln.Report

import Prettyprinter ((<+>))

type ParseEnv = DiagnosticEnv ColnCode

top :: ParseEnv -> [Ntn] -> IO Globals
top e = foldlM (decl' e) emptyGlobals

topFromText :: Reporter ColnCode -> File -> IO Globals
topFromText r f = do
  ts <- N.lex lexConfig (contramap LexerCode r) f
  ns <- N.read readConfig (contramap ReaderCode r) f ts
  top (DiagnosticEnv r f) ns

decl' :: ParseEnv -> Globals -> Ntn -> IO Globals
decl' e g n = do
  try (decl e g n) >>= \case
    Right g' -> pure g'
    Left (_ :: FailException) -> pure g

decl :: ParseEnv -> Globals -> Ntn -> IO Globals
decl e g (N.Decl "theory" n _) = do
  (x, ge) <- idef e g (M.univ TheoryU) n
  pure $ addGlobalEntry x ge g
decl e g (N.Decl "def" n _) = do
  (x, ge) <- def e g n
  pure $ addGlobalEntry x ge g
decl e g (N.Block "realm" (Just head) body _) = do
  (x, r) <- realm e g head body
  pure $ addRealm x r g
decl e _ n = unexpectedNotation e n "top-level declaration"

definition :: ParseEnv -> Ntn -> IO (Ntn, Ntn)
definition _ (N.Infix n0 (N.Keyword ":=" _) n1) = pure (n0, n1)
definition e n = unexpectedNotation e n "notation of the form `<head> := <body>`"

annot :: ParseEnv -> Ntn -> IO (Ntn, Ntn)
annot _ (N.Infix n0 (N.Keyword ":" _) n1) = pure (n0, n1)
annot e n = unexpectedNotation e n "type-annotated expression, e.g. `<pattern> : <type>`"

debugCommand :: ParseEnv -> Span -> Name -> Ntn -> IO DebugCommand
debugCommand e _ "showtype" n = do
  s <- syn e "argument to showtype" n
  pure $ ShowType (N.span n) s
debugCommand e _ "showtypeb" n = do
  s <- syn e "argument to showtypeb" n
  pure $ ShowTypeBehavior (N.span n) s
debugCommand e _ "showlevel" n = do
  ty <- typ e n
  pure $ ShowLevel (N.span n) ty
debugCommand e _ "expand" n = do
  s <- syn e "argument to expand" n
  pure $ Expand (N.span n) s
debugCommand e sp x _ = unknownCommand e sp x

fieldDecl :: ParseEnv -> Ntn -> IO Record.FieldDeclaration
fieldDecl e (N.Infix (N.Ident x _) (N.Keyword ":" _) n) =
  Record.FieldDeclaration x <$> typ e n
fieldDecl e (N.Decl c n sp) =
  Record.FieldDeclarationDebug <$> debugCommand e sp c n
fieldDecl e n = unexpectedNotation e n "field declaration of the form `<fieldname> : <type>`"

fieldSetting :: (V.HasEvaluation c) => ParseEnv -> Ntn -> IO (Record.FieldSetting c)
fieldSetting e (N.Infix (N.Ident x sp) (N.Keyword ":=" _) body) =
  Record.FieldSetting x <$> chk e body <*> pure sp
fieldSetting e n = unexpectedNotation e n "field setting of the form `<fieldname> := <expr>`"

ident :: ParseEnv -> Ntn -> IO Name
ident _ (N.Ident x _) = pure x
ident e n = unexpectedNotation e n "identifier"

argBinding :: ParseEnv -> Ntn -> IO (Span, Name, Ntn)
argBinding e n@(N.Infix n0 (N.Keyword ":" _) n1) = do
  x <- ident e n0
  pure (N.span n, x, n1)
argBinding e n = unexpectedNotation e n "argument binding of the form `<name> : <type>`"

unpackArgs :: ParseEnv -> Ntn -> IO (Name, [(Span, Name, Ntn)])
unpackArgs e (N.Group (xN :| argsN)) = do
  x <- ident e xN
  args <- mapM (argBinding e) argsN
  pure (x, args)

def :: ParseEnv -> Globals -> Ntn -> IO (Name, GlobalEntry)
def e g n = do
  (head_n, body_n) <- definition e n
  (pat_n, ty_n) <- annot e head_n
  (name, args) <- unpackArgs e pat_n
  ret_typ <- typ e ty_n
  body_chk <- chk e body_n
  (ty_j, term_j) <- withArgs e args (ret_typ, body_chk)
  let tyElabE = emptyElabEnv (contramap ElaboratorCode e) g
  ty <- ty_j.elab tyElabE
  let elabE = emptyElabEnvFor (contramap ElaboratorCode e) g name ty.val
  term <- term_j.elab elabE ty.val
  let entry = M.mkGlobal name ty.val term
  pure (name, entry)

idef :: ParseEnv -> Globals -> M.Ty N -> Ntn -> IO (Name, GlobalEntry)
idef e g ret_ty n = do
  (pat_n, body_n) <- definition e n
  (name, args) <- unpackArgs e pat_n
  body_chk <- chk e body_n
  (ty_j, term_j) <- withArgs e args $ (Typ \_ -> pure ret_ty, body_chk)
  let tyElabE = emptyElabEnv (contramap ElaboratorCode e) g
  ty <- ty_j.elab tyElabE
  let elabE = emptyElabEnvFor (contramap ElaboratorCode e) g name ty.val
  term <- term_j.elab elabE ty.val
  let entry = M.mkGlobal name ty.val term
  pure (name, entry)

realmHead :: ParseEnv -> Ntn -> IO (Name, Ntn)
realmHead _ (N.Infix (N.Ident x _) (N.Keyword "@" _) n) = pure (x, n)
realmHead e n = unexpectedNotation e n "realm head"

realm :: ParseEnv -> Globals -> Ntn -> [Ntn] -> IO (Name, Realm)
realm e g head _defs = do
  (x, theory_n) <- realmHead e head
  theory_typ <- typ e theory_n
  theory <- theory_typ.elab (emptyElabEnv (contramap ElaboratorCode e) g)
  let (gt, root) = layoutTop x theory.val
  pure (x, Realm gt root.val theory.val)

withArgs :: (V.HasEvaluation c) => ParseEnv -> [(Span, Name, Ntn)] -> (Typ N, Chk c) -> IO (Typ N, Chk c)
withArgs e args base = foldrM go base args
 where
  go :: (V.HasEvaluation c) => (Span, Name, Ntn) -> (Typ N, Chk c) -> IO (Typ N, Chk c)
  go (sp, name, n) (t, c) = do
    argtyp <- typ e n
    pure $
      ( Function.formation sp (Function.Named name argtyp) t
      , Function.intro sp name c
      )

fromSynN :: (V.HasEvaluation c) => Syn N -> Judgment c
fromSynN @c s = case V.scase @c of
  SNominative -> FromSyn s
  SDescriptive -> FromSyn $ Syn \e -> do
    (a, m) <- s.elab (e{target = TargetAnonymous})
    pure (a, M.is m)

fromTypN :: (V.HasEvaluation c) => Typ N -> Judgment c
fromTypN @c t = case V.scase @c of
  SNominative -> FromTyp t
  SDescriptive -> FromTyp $ Typ \e -> do
    m <- t.elab (e{target = TargetAnonymous})
    pure $ M.isTy m

fromTypD :: (V.HasEvaluation c) => ParseEnv -> Span -> Typ D -> IO (Judgment c)
fromTypD @c e sp t = case V.scase @c of
  SNominative -> do
    let msg = "expected nominative type, got descriptive type"
    failWith e sp (ParserCode UnexpectedDescriptive) msg
  SDescriptive -> pure $ FromTyp t

expr :: (V.HasEvaluation c) => ParseEnv -> Ntn -> IO (Judgment c)
expr e n = case n of
  N.Ident name s -> pure $ fromSynN $ Variable.find s name
  N.Juxt n0 n1 -> do
    s <- syn e "target of elimination" n0
    fromSynN <$> elim e s n1
  N.Keyword "Set" _ -> pure $ fromTypN $ Universe.formation SetU
  N.Keyword "Prop" _ -> pure $ fromTypN $ Universe.formation PropU
  N.Keyword "Int" _ -> pure $ fromTypN $ Builtin.formation BuiltinInt
  N.Keyword "String" _ -> pure $ fromTypN $ Builtin.formation BuiltinString
  N.Infix arg n@(N.Keyword "->" _) body ->
    fromTypN <$> (Function.formation (N.span n) <$> binder e arg <*> typ e body)
  N.Infix arg n@(N.Keyword "=>" _) body ->
    FromChk "lambda expression"
      <$> (Function.intro (N.span n) <$> ident e arg <*> chk e body)
  n@(N.Infix lhs (N.Keyword "=" _) rhs) ->
    fromTypN
      <$> ( Equality.formation (N.span n)
              <$> syn e "term in equality" lhs
              <*> syn e "term in equality" rhs
          )
  N.Block "sig" Nothing ns _ -> do
    t <- Record.formation <$> traverse (fieldDecl e) ns
    fromTypD e (N.span n) t
  N.Block "struct" Nothing ns s ->
    FromChk "struct expression" <$> (Record.intro s <$> traverse (fieldSetting e) ns)
  N.Int i _ -> pure $ fromSynN $ Builtin.intro $ LitInt i
  N.String s _ -> pure $ fromSynN $ Builtin.intro $ LitString s
  n -> unexpectedNotation e n "expression"

syn :: (V.HasEvaluation c) => ParseEnv -> DDoc -> Ntn -> IO (Syn c)
syn e use n = intoSyn use (N.span n) <$> expr e n

chk :: (V.HasEvaluation c) => ParseEnv -> Ntn -> IO (Chk c)
chk e n = intoChk (N.span n) <$> expr e n

typ :: ParseEnv -> Ntn -> IO (Typ N)
typ e n = intoTyp (N.span n) <$> expr e n

elim :: ParseEnv -> Syn N -> Ntn -> IO (Syn N)
elim e j = \case
  N.Field x s -> pure $ Record.elim s j x
  arg -> Function.elim (N.span arg) j <$> chk e arg

binder :: ParseEnv -> Ntn -> IO Function.Binder
binder e = \case
  N.Infix name (N.Keyword ":" _) arg ->
    Function.Named <$> ident e name <*> typ e arg
  n -> Function.Anonymous <$> typ e n

unexpectedNotation :: ParseEnv -> Ntn -> DDoc -> IO a
unexpectedNotation e n c = do
  let msg = "unexpected notation for" <+> c <> ":" <+> N.head n
  failWith e (N.span n) (ParserCode UnexpectedNotation) msg

unknownCommand :: ParseEnv -> Span -> Name -> IO a
unknownCommand e sp x = do
  let msg = "unknown command:" <+> dpretty x
  failWith e sp (ParserCode UnknownCommand) msg
