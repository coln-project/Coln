{-# LANGUAGE TypeAbstractions #-}
module Coln.Frontend.Parser.Expr where

import FNotation (Ntn)
import FNotation qualified as N

import Coln.Common
import Coln.Core
import Coln.Core.Memoed qualified as M
import Coln.Core.Value qualified as V
import Coln.Elaborator.Coercion
import Coln.Elaborator.Debug
import Coln.Elaborator.Environment
import Coln.Elaborator.Judgment
import Coln.Elaborator.Rules.Builtin qualified as Builtin
import Coln.Elaborator.Rules.Equality qualified as Equality
import Coln.Elaborator.Rules.Function qualified as Function
import Coln.Elaborator.Rules.Initial qualified as Initial
import Coln.Elaborator.Rules.Record qualified as Record
import Coln.Elaborator.Rules.Universe qualified as Universe
import Coln.Elaborator.Rules.Variable qualified as Variable

import Coln.Frontend.Diagnostics

type ParserEnv = DiagnosticEnv ParserCode

debugCommand :: ParserEnv -> Span -> Name -> Ntn -> IO DebugCommand
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

binder :: ParserEnv -> Ntn -> IO Function.Binder
binder e = \case
  N.Infix name (N.Keyword ":" _) arg ->
    Function.Named <$> ident e name <*> typ e arg
  n -> Function.Anonymous <$> typ e n

fieldDecl :: ParserEnv -> Ntn -> IO Record.FieldDeclaration
fieldDecl e (N.Infix (N.Ident x _) (N.Keyword ":" _) n) =
  Record.FieldDeclaration x <$> typ e n
fieldDecl e (N.Decl c n sp) =
  Record.FieldDeclarationDebug <$> debugCommand e sp c n
fieldDecl e n = unexpectedNotation e n "field declaration of the form `<fieldname> : <type>`"

fieldSetting :: (V.HasEvaluation c) => ParserEnv -> Ntn -> IO (Record.FieldSetting c)
fieldSetting e (N.Infix (N.Ident x sp) (N.Keyword ":=" _) body) =
  Record.FieldSetting x <$> chk e body <*> pure sp
fieldSetting e n = unexpectedNotation e n "field setting of the form `<fieldname> := <expr>`"

ident :: ParserEnv -> Ntn -> IO Name
ident _ (N.Ident x _) = pure x
ident e n = unexpectedNotation e n "identifier"

unexpectedNotation :: ParserEnv -> Ntn -> DDoc -> IO a
unexpectedNotation e n c = do
  let msg = "unexpected notation for" <+> c <> ":" <+> N.head n
  failWith e (N.span n) UnexpectedNotation msg

unknownCommand :: ParserEnv -> Span -> Name -> IO a
unknownCommand e sp x = do
  let msg = "unknown command:" <+> dpretty x
  failWith e sp UnknownCommand msg

fromSynN :: (V.HasEvaluation c) => Syn N -> Judgment c
fromSynN @c s = case V.scase @c of
  SNominative -> FromSyn s
  SDescriptive -> FromSyn $ Syn \e -> do
    (a, m) <- s.elab (e{target = TargetAnonymous})
    pure (a, M.is m)
  
fromSynD :: (V.HasEvaluation c) => ParserEnv -> Span -> Syn D -> IO (Judgment c)
fromSynD @c e sp s = case V.scase @c of
  SNominative -> do
    let msg = "expected nominative expression, got descriptive expression"
    failWith e sp UnexpectedDescriptive msg
  SDescriptive -> pure $ FromSyn s

fromTypN :: (V.HasEvaluation c) => Typ N -> Judgment c
fromTypN @c t = case V.scase @c of
  SNominative -> FromTyp t
  SDescriptive -> FromTyp $ Typ \e -> do
    m <- t.elab (e{target = TargetAnonymous})
    pure $ M.isTy m

fromTypD :: (V.HasEvaluation c) => ParserEnv -> Span -> Typ D -> IO (Judgment c)
fromTypD @c e sp t = case V.scase @c of
  SNominative -> do
    let msg = "expected nominative type, got descriptive type"
    failWith e sp UnexpectedDescriptive msg
  SDescriptive -> pure $ FromTyp t

expr :: (V.HasEvaluation c) => ParserEnv -> Ntn -> IO (Judgment c)
expr e n = case n of
  N.Ident name s -> pure $ fromSynN $ Variable.find s name
  N.Juxt (N.Keyword "init" _) n -> do
    t <- typ e n
    fromSynD e (N.span n) (Initial.create (N.span n) t)
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

syn :: (V.HasEvaluation c) => ParserEnv -> DDoc -> Ntn -> IO (Syn c)
syn e use n = intoSyn use (N.span n) <$> expr e n

chk :: (V.HasEvaluation c) => ParserEnv -> Ntn -> IO (Chk c)
chk e n = intoChk (N.span n) <$> expr e n

typ :: ParserEnv -> Ntn -> IO (Typ N)
typ e n = intoTyp (N.span n) <$> expr e n

elim :: ParserEnv -> Syn N -> Ntn -> IO (Syn N)
elim e j = \case
  N.Field x s -> pure $ Record.elim s j x
  arg -> Function.elim (N.span arg) j <$> chk e arg

