module Coln.Elaborator.Parse where

import Data.Foldable
import Data.Functor.Contravariant (contramap)
import FNotation (Ntn)
import FNotation qualified as N

import Coln.Common
import Coln.Core.Params
import Coln.Core.Memoed qualified as M
import Coln.Core.Syntax qualified as S
import Coln.Elaborator.Diagnostics
import Coln.Elaborator.Environment
import Coln.Elaborator.Judgment
import Coln.Elaborator.Rules.Builtin qualified as Builtin
import Coln.Elaborator.Rules.Equality qualified as Equality
import Coln.Elaborator.Rules.Function qualified as Function
import Coln.Elaborator.Rules.Record qualified as Record
import Coln.Elaborator.Rules.Universe qualified as Universe
import Coln.Elaborator.Rules.Variable qualified as Variable
import Coln.Report

import Prettyprinter ((<+>))

type ParseEnv = DiagnosticEnv ColnCode

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
def e g n = do
  (head_n, body_n) <- definition n
  (pat_n, ty_n) <- annot head_n
  (name, args_n) <- unpackArgs pat_n
  let elabE = elabEnvNamed name
  (ty_j, term_j) <- withArgs e g args $ do
    bty_j <- expr e g ty_n
    body_j <- expr e g body_n
    pure (bty_j, body j)
  ty <- typ ty_j elabE
  term <- chk term_j ty elabE
  entry <- M.mkGlobal name ty.val term
  pure (name, entry)

idef :: ParseEnv -> S.Globals -> M.Ty N -> Ntn -> IO (Name, S.GlobalEntry)
idef e g bty_j n = do
  (head_n, body_n) <- definition n
  (pat_n, ty_n) <- annot head_n
  (name, args_n) <- unpackArgs pat_n
  let elabE = elabEnvNamed name
  (ty_j, term_j) <- withArgs e g args $ do
    body_j <- expr e g body_n
    pure (bty_j, body_j)
  ty <- typ ty_j elabE
  term <- chk term_j ty elabE
  entry <- M.mkGlobal name ty.val term
  pure (name, entry)

withArgs :: ParseEnv -> [(Span, Name, Ntn)] -> IO (Judgment N, Judgment c) -> IO (Judgment N, Judgment c)
withArgs e = (=<<) . (flip $ foldrM go)
  where
    go :: (Span, Name, Ntn) -> (Judgment N, Judgment c) -> IO (Judgment N, Judgment c)
    go (sp, name, n) (ty_j, term_j) = do
      argty_j <- expr e n
      let fnty_j = Function.formation sp (Function.Named name argty_j) ty_j
      let fnterm_j = Function.intro sp name term_j
      pure (fnty_j, fnterm_j)

expr :: ParseEnv -> Ntn -> IO (Judgment c)
expr e = \case
  N.Ident name s -> pure $ Variable.find s name
  N.Juxt n0 n1 -> expr e n0 >>= elim e n1
  N.Keyword "Set" sp -> pure $ Universe.formation sp SetU
  N.Keyword "Prop" sp -> pure $ Universe.formation sp SetU
  N.Keyword "Int" sp -> pure $ Builtin.formation sp BuiltinInt
  N.Keyword "String" sp -> pure $ Builtin.formation sp BuiltinString
  N.Infix arg n@(N.Keyword "->" _) body ->
    Function.formation (N.span n) <$> binder e arg <*> expr e body
  N.Infix arg n@(N.Keyword "=>" _) body ->
    Function.intro (N.span n) <$> expr e arg <*> expr e body
  n@(N.Infix lhs (N.Keyword "=" _) rhs) ->
    Equality.formation (N.span n) <$> expr e lhs <*> expr e rhs
  N.Block "sig" Nothing ns s ->
    Record.formation s <$> traverse (fieldDecl e) ns
  N.Block "struct" Nothing ns s ->
    Record.intro s <$> traverse (fieldSetting e) ns
  N.Int i sp -> pure $ Builtin.intro sp $ LitInt i
  N.String s sp -> pure $ Builtin.intro sp $ LitString s
  n -> unexpectedNotation e n "expression"

elim :: ParseEnv -> Judgment N -> Ntn -> IO (Judgment c)
elim e x = \case
  N.Field n s -> pure $ Record.elim s x n
  arg -> Function.elim (N.span arg) x <$> expr e arg
  
binder :: ParseEnv -> Ntn -> IO Function.Binder
binder e = \case
  N.Infix name (N.Keyword ":" _) arg ->
    Function.Named <$> ident e name <*> expr e arg
  n -> Function.Anonymous <$> expr n

ident :: ParseEnv -> Ntn -> IO Function.Binder
ident e = \case
  N.Ident name _ -> pure name
  n -> unexpectedNotation e n "notation"

unexpectedNotation :: ParseEnv -> Ntn -> DDoc -> IO a
unexpectedNotation e n c = do
  let msg = "unexpected notation for" <+> c <> ":" <+> N.head n
  failWith e (N.span n) (ReparseCode UnexpectedNotation) msg
