module Geolog.Elaborator.Parse where

import Data.Foldable
import Data.Functor.Contravariant (contramap)
import FNotation (Ntn)
import FNotation qualified as N

import Geolog.Common
import Geolog.Core.Params
import Geolog.Core.Syntax qualified as S
import Geolog.Elaborator.Diagnostics
import Geolog.Elaborator.Environment
import Geolog.Elaborator.Rules.Builtin qualified as Builtin
import Geolog.Elaborator.Rules.Equality qualified as Equality
import Geolog.Elaborator.Rules.Function qualified as Function
import Geolog.Elaborator.Rules.Record qualified as Record
import Geolog.Elaborator.Rules.Universe qualified as Universe
import Geolog.Elaborator.Rules.Variable qualified as Variable
import Geolog.Report

type ParseEnv = DiagnosticEnv GeologCode

top :: ParseEnv -> [Ntn] -> IO S.Globals
top e = foldlM (decl' e) emptyGlobals

decl' :: ParseEnv -> Ntn -> S.Globals -> IO S.Globals
decl' e n g = do
  (name, entry) <- decl e g n
  pure $ S.addGlobalEntry name entry g

decl :: ParseEnv -> S.Globals -> Ntn -> IO (Name, S.GlobalEntry)
decl e g (N.Decl "theory" n _) = idef e g (Universe.formation TheoryU) n
decl e g (N.Decl "set" n _) = idef e g (Universe.formation SetU) n
decl e g (N.Decl "def" n _) = def e g n
decl e g n = unexpectedNotation e n "top-level declaration"

def :: ParseEnv -> S.Globals -> Ntn -> IO (Name, S.GlobalEntry)
def e g = do
  (head_n, body_n) <- definition n
  (pat_n, ty_n) <- annot head_n
  (name, args_n) <- unpackArgs pat_n
  let elabE = elabEnvNamed name
  (ty_j, term_j) <- withArgs e g args $ do
    bty_j <- expr e g ty_n
    body_j <- expr e g body_n
    pure (bty_j, body j)
  ty <- typ j elabE
  term <- chk j ty elabE
  (stx, val) <- mkGlobal term
  pure (name, S.GlobalEntry (stx, val, ty.val))

idef :: ParseEnv -> S.Globals -> Ntn -> M.Ty K -> IO (Name, S.GlobalEntry)
idef e g bty_j = do
  (head_n, body_n) <- definition n
  (pat_n, ty_n) <- annot head_n
  (name, args_n) <- unpackArgs pat_n
  let elabE = elabEnvNamed name
  (ty_j, term_j) <- withArgs e g args $ do
    body_j <- expr e g body_n
    pure (bty_j, body_j)
  ty <- typ j elabE
  term <- chk j ty elabE
  (stx, val) <- mkGlobal term
  pure (name, S.GlobalEntry (stx, val, ty.val))

withArgs :: ParseEnv -> [(Span, Name, Ntn)] -> IO (Judgment N, Judgment c) -> IO (Judgment K, Judgment c)
withArgs e = flip foldrM go
  where
    go (sp, name, n) (ty_j, term_j) = do
      argty_j <- expr n
      let fnty_j = Function.formation (Named name argty_j) ty_j
      let fnterm_j = Function.intro sp name term_j
      pure (fnty_j, fnterm_j)

expr :: ParseEnv -> Ntn -> IO (Judgment c)
expr e = \case
  N.Ident name s -> pure $ Variable.rule s name
  N.Juxt n0 n1 -> expr e n0 >>= elim e n1
  N.Keyword "Set" -> pure $ Universe.formation Set_U
  N.Keyword "Prop" -> pure $ Universe.formation Set_U
  N.Keyword "Int" -> pure $ Builtin.formation BuiltinInt
  N.Keyword "String" -> pure $ Builtin.formation BuiltinString
  N.Infix arg (N.Keyword "->" _) body ->
    Function.formation <$> binder e arg <*> expr e body
  N.Infix arg n@(N.Keyword "=>" _) body ->
    Function.intro (N.span n) <$> expr e arg <*> expr e body
  N.Infix lhs (N.Keyword "=" _) rhs ->
    Equality.formation <$> expr e lhs <*> expr e rhs
  N.Block "sig" Nothing ns _ ->
    Record.formation <$> traverse (fieldDecl e) ns
  N.Block "struct" Nothing ns s ->i
    Record.intro s <$> traverse (fieldSetting e) ns
  N.Int i _ -> pure $ Builtin.intro $ LitInt i
  N.String s _ -> pure $ Builtin.intro $ LitString s
  n -> unexpectedNotation n "expression"

elim :: ParseEnv -> Judgment N -> Ntn -> IO (Judgment c)
elim e x = \case
  N.Field n s -> pure $ Record.elim s x n
  arg -> Function.elim (N.span arg) x <$> expr arg
  
binder :: ParseEnv -> Ntn -> IO Function.Binder
binder e = \case
  N.Infix name (N.Keyword ":" _) arg ->
    Function.Named <$> ident name <*> expr body
  n -> Function.Anonymous <$> expr n


