-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Frontend.Parser.Top where

import Control.Exception (try)
import Data.Foldable
import Data.Functor.Contravariant (contramap)
import Data.List.NonEmpty (NonEmpty (..))

import Data.Map.Ordered qualified as OMap
import FNotation (Ntn)
import FNotation qualified as N
import Prettyprinter

import Coln.Common
import Coln.Core
import Coln.Core.Memoed qualified as M
import Coln.Core.Value qualified as V
import Coln.Diagnostics
import Coln.Elaborator.Environment as Environment
import Coln.Elaborator.Judgment
import Coln.Elaborator.Rules.Function qualified as Function
import Coln.Frontend.Diagnostics
import Coln.Frontend.Notation
import Coln.Frontend.Parser.Expr

definition :: ParserEnv -> Ntn -> IO (Ntn, Ntn)
definition _ (N.Infix n0 (N.Keyword ":=" _) n1) = pure (n0, n1)
definition e n = unexpectedNotation e n "notation of the form `<head> := <body>`"

annot :: ParserEnv -> Ntn -> IO (Ntn, Ntn)
annot _ (N.Infix n0 (N.Keyword ":" _) n1) = pure (n0, n1)
annot e n = unexpectedNotation e n "type-annotated expression, e.g. `<pattern> : <type>`"

argBinding :: ParserEnv -> Ntn -> IO (Span, Mode, Name, Typ N)
argBinding e n@(N.Infix n0 (N.Keyword ":" _) n1) = do
  (m, x) <- modalIdent e Conjunctive n0
  a <- typ e n1
  pure (N.span n, m, x, a)
argBinding e n = unexpectedNotation e n "argument binding of the form `<name> : <type>`"

unpackArgs :: ParserEnv -> Ntn -> IO (Name, [(Span, Mode, Name, Typ N)])
unpackArgs e (N.Group (xN :| argsN)) = do
  x <- ident e xN
  args <- mapM (argBinding e) argsN
  pure (x, args)

withArgs :: (V.HasEvaluation c) => [(Span, Mode, Name, Typ N)] -> (Typ N, Chk c) -> (Typ N, Chk c)
withArgs args base = foldr go base args
 where
  go :: (V.HasEvaluation c) => (Span, Mode, Name, Typ N) -> (Typ N, Chk c) -> (Typ N, Chk c)
  go (sp, m, name, a) (t, c) =
    ( Function.formation sp (Function.Named m name a) t
    , Function.intro sp name c
    )

theory :: ParserEnv -> Ntn -> IO (Name, Typ N, Chk D)
theory e n = do
  (pat_n, body_n) <- definition e n
  (name, args) <- unpackArgs e pat_n
  body <- chk e body_n
  let (ty, tm) = withArgs args (Typ $ \_ -> pure $ M.univ TheoryU, body)
  pure $ (name, ty, tm)

def :: ParserEnv -> Ntn -> IO (Name, Typ N, Chk D)
def e n = do
  (head_n, body_n) <- definition e n
  (pat_n, ty_n) <- annot e head_n
  (name, args) <- unpackArgs e pat_n
  returnTyp <- typ e ty_n
  body <- chk e body_n
  let (ty, tm) = withArgs args (returnTyp, body)
  pure (name, ty, tm)

elabDefinition :: DiagnosticEnv ElaboratorCode -> Globals -> Mode -> (Name, Typ N, Chk D) -> IO (Definition Global)
elabDefinition e g m (x, ty, tm) = do
  let tyE = emptyElabEnv e g m
  a <- ty.elab tyE
  let tmE = emptyElabEnvFor e g m x a.val
  t <- tm.elab tmE a.val
  let v = V.reflect (V.GlobalVar x v) V.Id a.val (Just t.val)
  let entry = Definition t a.val v m
  pure entry

mode :: ParserEnv -> Span -> [Name] -> IO Mode
mode _ _ [] = pure Conjunctive
mode _ _ ["ind"] = pure Inductive
mode e sp ms = do
  let msg = "unknown modifiers" <+> hsep (dpretty <$> ms)
  failWith e sp UnknownModifiers msg

decl :: DiagnosticEnv ColnCode -> Globals -> Ntn -> IO Globals
decl e g (N.MDecl ms "theory" n sp) = do
  m <- mode (contramap ParserCode e) sp ms
  (x, t, c) <- theory (contramap ParserCode e) n
  ge <- elabDefinition (contramap ElaboratorCode e) g m (x, t, c)
  pure $ addDefinition x ge g
decl e g (N.MDecl ms "def" n sp) = do
  m <- mode (contramap ParserCode e) sp ms
  (x, t, c) <- def (contramap ParserCode e) n
  ge <- elabDefinition (contramap ElaboratorCode e) g m (x, t, c)
  pure $ addDefinition x ge g
decl e g (N.Block "realm" (Just head) body _) = do
  (x, r) <- realm e g head body
  pure $ addRealm x r g
decl e _ n = unexpectedNotation (contramap ParserCode e) n "top-level declaration"

realmHead :: ParserEnv -> Ntn -> IO (Name, Ntn)
realmHead _ (N.Infix (N.Ident x _) (N.Keyword "@" _) n) = pure (x, n)
realmHead e n = unexpectedNotation e n "realm head"

realm :: DiagnosticEnv ColnCode -> Globals -> Ntn -> [Ntn] -> IO (Name, Realm)
realm e g head def_ns = do
  (x, theory_n) <- realmHead (contramap ParserCode e) head
  theory_typ <- typ (contramap ParserCode e) theory_n
  theory <- theory_typ.elab (emptyElabEnv (contramap ElaboratorCode e) g Inductive)
  defs <- realmDecls e g theory.val def_ns
  pure (x, Realm theory defs)

elabRealmDefinition :: ElabEnv N -> Mode -> (Typ N, Chk D) -> IO (Definition Local)
elabRealmDefinition e m (ty, tm) = do
  let e' = e{scope = e.scope{Environment.mode = m}}
  a <- ty.elab e'
  let var = V.LocalVar (FId e'.scope.len)
  let e'' = e'{target = TargetNamed $ V.BareNeutral var V.Id}
  t <- tm.elab e'' a.val
  let v = V.reflect var V.Id a.val (Just t.val)
  pure $ Definition t a.val v m

realmDecl :: DiagnosticEnv ColnCode -> ElabEnv N -> Ntn -> IO (Name, Definition Local)
realmDecl de e (N.MDecl ms "def" n sp) = do
  m <- mode (contramap ParserCode de) sp ms
  (x, t, c) <- def (contramap ParserCode de) n
  d <- elabRealmDefinition e m (t, c)
  pure (x, d)
realmDecl de _ n = unexpectedNotation (contramap ParserCode de) n "realm declaration"

realmDecls :: DiagnosticEnv ColnCode -> Globals -> V.Ty N -> [Ntn] -> IO (OMap Name (Definition Local))
realmDecls de g theory ns = do
  let e0 = emptyElabEnv (contramap ElaboratorCode de) g Conjunctive
  let e = e0{scope = bind "root" theory Conjunctive e0.scope}
  let addDecl (e', ds) n = do
        (x, d) <- realmDecl de e' n
        pure
          ( e'{scope = let_ x d.reflected d.ty d.mode e'.scope}
          , ds OMap.>| (x, d)
          )
  (_, ds) <- foldlM addDecl (e, OMap.empty) ns
  pure $ ds

tryDecl :: DiagnosticEnv ColnCode -> Globals -> Ntn -> IO Globals
tryDecl e g n = do
  try (decl e g n) >>= \case
    Right g' -> pure g'
    Left (_ :: FailException) -> pure g

top :: DiagnosticEnv ColnCode -> [Ntn] -> IO Globals
top e = foldlM (tryDecl e) emptyGlobals

topFromText :: Reporter ColnCode -> File -> IO Globals
topFromText r f = do
  ts <- N.lex lexConfig (contramap LexerCode r) f
  ns <- N.read readConfig (contramap ReaderCode r) f ts
  top (DiagnosticEnv r f) ns
