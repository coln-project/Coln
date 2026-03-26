module Geolog.Elaborator where

import Control.Exception
import Control.Monad (unless)
import Data.Map (Map)
import Data.Map qualified as Map
import Diagnostician
import FNotation (Name, Ntn)
import FNotation qualified as N
import Geolog.Common
import Geolog.Core
import Geolog.CoreOperations
import Geolog.Pretty
import Prettyprinter
import Prelude hiding (head, lookup)

-- Diagnostic codes
--------------------------------------------------------------------------------

data ElaboratorCode
  = FailedConversion
  | NotInScope
  | UnsupportedInPotentialMode
  | UnsupportedInKineticMode
  | ProjectionFromNonRecord
  | NoSuchField
  | ApplicationOfNonPi
  | MustChk
  | UnexpectedNotation
  | UnexpectedTuple
  | UnexpectedLambda
  | UnexpectedField
  | WrongNumberOfFields
  | WrongLevel
  | EqualityUnsupportedAtLevel
  deriving (Eq, Ord)

elaboratorCodeTable :: Map ElaboratorCode CodeMeta
elaboratorCodeTable =
  Map.fromList
    [ (FailedConversion, CodeMeta 0 SError Nothing),
      (NotInScope, CodeMeta 1 SError Nothing),
      (UnsupportedInPotentialMode, CodeMeta 2 SError Nothing),
      (UnsupportedInKineticMode, CodeMeta 3 SError Nothing),
      (ProjectionFromNonRecord, CodeMeta 4 SError Nothing),
      (NoSuchField, CodeMeta 5 SError Nothing),
      (ApplicationOfNonPi, CodeMeta 6 SError Nothing),
      (MustChk, CodeMeta 7 SError Nothing),
      (UnexpectedNotation, CodeMeta 8 SError Nothing),
      (UnexpectedTuple, CodeMeta 9 SError Nothing),
      (UnexpectedLambda, CodeMeta 10 SError Nothing),
      (UnexpectedField, CodeMeta 11 SError Nothing),
      (WrongNumberOfFields, CodeMeta 12 SError Nothing),
      (WrongLevel, CodeMeta 13 SError Nothing),
      (EqualityUnsupportedAtLevel, CodeMeta 14 SError Nothing)
    ]

-- Elaboration implicits
--------------------------------------------------------------------------------

data Ctx = Ctx
  { shape :: CtxShape,
    elts :: Bwd (ElV K),
    types :: Bwd (TyV K)
  }

emptyCtx :: Ctx
emptyCtx = Ctx mempty mempty mempty

data DiagnosticCtx = DiagnosticCtx
  { reporter :: ReporterFor ElaboratorCode,
    file :: File
  }

type DiagnosticCtxArg = (?diagnosticCtx :: DiagnosticCtx)

type ElabArgs = (GlobalEnvArg, DiagnosticCtxArg)

bind :: Name -> TyV K -> Ctx -> Ctx
bind x a c =
  let v = local a (FId c.shape.length)
   in let_ x v a c

withBound :: Name -> TyV K -> Ctx -> (ElV K -> Ctx -> a) -> a
withBound x a c action =
  let v = local a (FId c.shape.length)
      c' = let_ x v a c
   in action v c'

let_ :: Name -> ElV K -> TyV K -> Ctx -> Ctx
let_ x v a c = Ctx (c.shape ++> x) (c.elts :> v) (c.types :> a)

report :: (DiagnosticCtxArg) => Span -> ElaboratorCode -> DDoc -> IO ()
report s c m = do
  let n = Note (Just (SourceLoc ?diagnosticCtx.file s)) Nothing
  let d = Diagnostic c m [n]
  reportTo ?diagnosticCtx.reporter d

data ElabException = GiveUp
  deriving (Show)

instance Exception ElabException

failWith :: (DiagnosticCtxArg) => Span -> ElaboratorCode -> DDoc -> IO a
failWith s c m = do
  report s c m
  evaluate $ throw GiveUp

-- Glued values
--------------------------------------------------------------------------------

data Glued a b e = G
  { stx :: (a e),
    val :: (b e)
  }

type TyG = Glued TyS TyV

type ElG = Glued ElS ElV

instance Core ElG TyG where
  app (G ft fv) (G xt xv) = G (app ft xt) (app fv xv)
  proj (G t v) x = G (proj t x) (proj v x)
  code (G t v) = G (code t) (code v)
  decode (G t v) = G (decode t) (decode v)
  universe u = G (universe u) (universe u)
  builtinTy a = G (builtinTy a) (builtinTy a)
  lit l = G (lit l) (lit l)

-- Diagnostics
--------------------------------------------------------------------------------

notInScope :: (DiagnosticCtxArg) => Span -> Name -> IO a
notInScope s x = failWith s NotInScope $ "identifier" <+> dpretty x <+> "not in scope"

unsupportedInPotentialMode :: (DiagnosticCtxArg) => Span -> DDoc -> IO a
unsupportedInPotentialMode s feature =
  failWith s UnsupportedInPotentialMode $
    feature <+> "unsupported while elaborating a potential term"

unsupportedInKineticMode :: (DiagnosticCtxArg) => Span -> DDoc -> IO a
unsupportedInKineticMode s feature =
  failWith s UnsupportedInKineticMode $
    feature <+> "unsupported while elaborating a kinetic term"

mustChk :: (DiagnosticCtxArg) => Span -> DDoc -> IO a
mustChk s feature =
  failWith s MustChk $
    feature <+> "unsupported while in synthesis mode"

unexpectedNotation :: (DiagnosticCtxArg) => Ntn -> DDoc -> IO a
unexpectedNotation n c =
  failWith (N.span n) UnexpectedNotation $
    "unexpected notation for" <+> c <> ":" <+> N.head n

unexpectedTuple :: (DiagnosticCtxArg) => Span -> DDoc -> IO a
unexpectedTuple s a =
  failWith s UnexpectedTuple $
    "tried to check tuple notation at type" <+> a <+> "which is not a record type"

unexpectedLambda :: (DiagnosticCtxArg) => Span -> DDoc -> IO a
unexpectedLambda s a =
  failWith s UnexpectedLambda $
    "tried to check lambda notation at type" <+> a <+> "which is not a pi type"

conversionError :: (DiagnosticCtxArg) => Span -> DDoc -> DDoc -> DefEqCheckError -> IO a
conversionError s t t' e = do
  let convMessage = "synthesized" <+> t' <+> "while expecting" <+> t
  let convNote = Note Nothing (Just (pretty e))
  let locNote = Note (Just (SourceLoc ?diagnosticCtx.file s)) Nothing
  let d = Diagnostic FailedConversion convMessage [locNote, convNote]
  reportTo ?diagnosticCtx.reporter d
  evaluate $ throw GiveUp

wrongLevel :: (DiagnosticCtxArg) => Span -> IO a
wrongLevel s = failWith s WrongLevel "wrong level"

equalityUnsupportedAtLevel :: (DiagnosticCtxArg) => Span -> Level -> DDoc -> IO a
equalityUnsupportedAtLevel s l a =
  failWith s EqualityUnsupportedAtLevel $
    "equality types are unsupported at level" <+> dpretty l <> ", which is the inferred level of the type" <+> a

-- Helpers
--------------------------------------------------------------------------------

findLocal :: Ctx -> Name -> Maybe (ElG K, TyV K)
findLocal c x = go c.shape.values c.types c.elts 0
  where
    go :: Bwd Name -> Bwd (TyV K) -> Bwd (ElV K) -> BId -> Maybe (ElG K, TyV K)
    go (xs :> x') (as :> a) (vs :> v) i
      | x == x' = Just (G (LocalVar i) v, a)
      | otherwise = go xs as vs (i + 1)
    go _ _ _ _ = Nothing

findProj :: [Name] -> TeleV K -> [ElV K] -> Name -> Maybe (ElV K, TyV K)
findProj (x : xs) (TVCons a f) (v : vs) x'
  | x == x' = Just (v, a)
  | otherwise = findProj xs (f v) vs x'
findProj _ _ _ _ = Nothing

binding :: (DiagnosticCtxArg) => Ntn -> IO (Name, Ntn)
binding (N.Infix (N.Ident x _) (N.Keyword ":" _) n) = pure (x, n)
binding n = unexpectedNotation n "binding"

annot :: (DiagnosticCtxArg) => Ntn -> IO (Ntn, Ntn)
annot (N.Infix n1 (N.Keyword ":" _) n2) = pure (n1, n2)
annot n = unexpectedNotation n "type annotation"

unpackArgs :: (DiagnosticCtxArg) => Ntn -> IO (Name, [(Name, Ntn)])
unpackArgs (N.App n ns) = do
  x <- ident n
  args <- mapM binding ns
  pure (x, args)
unpackArgs (N.Ident x _) = pure (x, [])
unpackArgs n = unexpectedNotation n "application or identifier"

setting :: (DiagnosticCtxArg) => Name -> Ntn -> IO Ntn
setting x (N.Infix (N.Field x' sp) (N.Keyword "=" _) n')
  | x == x' = pure n'
  | otherwise =
      failWith sp UnexpectedField $
        "got field" <+> dpretty x' <> ", expected field" <+> dpretty x
setting _ n = unexpectedNotation n "record field"

ident :: (DiagnosticCtxArg) => Ntn -> IO Name
ident (N.Ident x _) = pure x
ident n = unexpectedNotation n "ident"

definition :: (DiagnosticCtxArg) => Ntn -> IO (Ntn, Ntn)
definition (N.Infix n1 (N.Keyword ":=" _) n2) = pure (n1, n2)
definition n = unexpectedNotation n "definition"

members :: (ElabArgs) => Ctx -> Universe -> [Ntn] -> IO ([Name], TeleS K)
members _ _ [] = pure ([], TSNil)
members c u (n : ns) = do
  (x, n') <- binding n
  ga <- typ c u n'
  (xs, as) <- members (bind x ga.val c) u ns
  pure (x : xs, TSCons ga.stx as)

elts :: (ElabArgs) => Ctx -> [Name] -> TeleV K -> [Ntn] -> IO ([ElS K], [ElV K])
elts _ [] TVNil [] = pure ([], [])
elts c (x : xs) (TVCons a f) (n : ns) = do
  n' <- setting x n
  G t v <- chkK c a n'
  (ts, vs) <- elts (let_ x v a c) xs (f v) ns
  pure (t : ts, v : vs)
elts _ _ _ _ = panic "fail earlier if we don't have right number of fields"

typ :: (ElabArgs) => Ctx -> Universe -> Ntn -> IO (TyG K)
typ c u n = decode <$> chkK c (VU u) n

synK :: (ElabArgs) => Ctx -> Ntn -> IO (ElG K, TyV K)
synK = syn SKinetic

synP :: (ElabArgs) => Ctx -> Ntn -> IO (ElG P, TyV K)
synP = syn SPotential

chkK :: (ElabArgs) => Ctx -> TyV K -> Ntn -> IO (ElG K)
chkK = chk SKinetic

chkP :: (ElabArgs) => Ctx -> TyV K -> Ntn -> IO (ElG P)
chkP = chk SPotential

guardDefEq :: (ElabArgs, DefEq a, Quote a b, DPrettyWithNames b) => Span -> Ctx -> a -> a -> c -> IO c
guardDefEq s c v0 v1 x =
  case defEq c.shape v0 v1 of
    Left err ->
      conversionError s (prtVal c.shape v0) (prtVal c.shape v1) err
    Right () -> pure x

-- syn and chk
--------------------------------------------------------------------------------

elim :: (ElabArgs) => Ctx -> ElG K -> TyV K -> Ntn -> IO (ElG K, TyV K)
elim _ g a (N.Field x s) =
  case behavesAs a of
    Just (VRecord _ xs te) -> case findProj xs te (coerceToFields g.val).values x of
      Just (v, a') -> pure (G (Proj g.stx x) v, a')
      Nothing ->
        failWith s NoSuchField $
          "no such field:" <+> dpretty x
    _ ->
      failWith
        s
        ProjectionFromNonRecord
        "target of attempted field projection is not of a record type"
elim c g a n =
  case behavesAs a of
    Just (VPi _ dom cod) -> do
      g' <- chkK c dom n
      pure (app g g', appClo cod g'.val)
    _ ->
      failWith
        (N.span n)
        ApplicationOfNonPi
        "target of attempted application is not of a pi type"

syn :: (ElabArgs) => SEnergy e -> Ctx -> Ntn -> IO (ElG e, TyV K)
syn SKinetic c (N.Ident x s) = case findLocal c x of
  Just res -> pure res
  Nothing -> case lookup ?globalEnv x of
    Just (KEntry _ v a) -> pure (G (GlobalVar x) v, a)
    Just (PEntry _ v a) -> pure (G (GlobalVar x) v', a)
      where
        v' = neu a (Global x) SId (Just v)
    Nothing -> notInScope s x
syn SPotential _ (N.Ident _ s) = unsupportedInPotentialMode s "variables"
syn SKinetic c (N.App n ns) = do
  (g, a) <- synK c n
  go g a ns
  where
    go g a [] = pure (g, a)
    go g a (n' : ns') = do
      (g', a') <- elim c g a n'
      go g' a' ns'
syn SPotential _ n@(N.App _ _) =
  unsupportedInPotentialMode (N.span n) "application"
syn SKinetic _ (N.Keyword "Query" _) =
  pure (code $ universe QueryU, universe TheoryU)
syn SPotential _ (N.Keyword "Query" s) =
  unsupportedInPotentialMode s "universes"
syn SKinetic c (N.Infix n1 (N.Keyword arr@("~>"; "->") _) nb) =
  let (domU, pv) = case arr of
        "~>" -> (PrimU, PrimTheory)
        "->" -> (QueryU, QueryTheory)
   in case n1 of
        (N.Infix (N.Ident x _) (N.Keyword ":" _) na) -> do
          ga <- typ c domU na
          gb <- typ (bind x ga.val c) TheoryU nb
          let t = Pi pv ga.stx (Abs x gb.stx)
          let v = VPi pv ga.val (Clo x (\w -> eval (c.elts :> w) gb.stx))
          pure (code $ G t v, universe TheoryU)
        na -> do
          ga <- typ c domU na
          gb <- typ c TheoryU nb
          let t = Pi pv ga.stx (AbsConst gb.stx)
          let v = VPi pv ga.val (CloConst gb.val)
          pure (code $ G t v, universe TheoryU)
syn SPotential _ n@(N.Infix _ (N.Keyword "->" _) _) =
  unsupportedInPotentialMode (N.span n) "pi types"
syn SKinetic c n@(N.Infix n0 (N.Keyword "=" _) n1) = do
  (g0, a0) <- synK c n0
  (g1, a1) <- synK c n1
  a <- guardDefEq (N.span n1) c a0 a1 a0
  unless (levelOf a == Query) $
    equalityUnsupportedAtLevel (N.span n) (levelOf a) (prtVal c.shape a)
  pure (code $ G (Eq (quote c.shape.length a) g0.stx g1.stx) (VEq a g0.val g1.val), universe QueryU)
syn _ _ (N.Keyword "Int" _) = pure (code $ builtinTy BuiltinInt, universe PrimU)
syn _ _ (N.Keyword "String" _) = pure (code $ builtinTy BuiltinString, universe PrimU)
syn SKinetic _ (N.String s _) = pure (lit $ LitString s, builtinTy BuiltinString)
syn SPotential _ (N.String _ sp) =
  unsupportedInPotentialMode sp "string literals"
syn SKinetic _ (N.Int i _) = pure (lit $ LitInt i, builtinTy BuiltinInt)
syn SPotential _ (N.Int _ sp) =
  unsupportedInPotentialMode sp "int literals"
syn _ _ n@(N.Infix _ (N.Keyword "=>" _) _) = mustChk (N.span n) "lambda syntax"
syn _ _ n@(N.Block "sig" _ _ _) = mustChk (N.span n) "signature"
syn _ _ n@(N.Block "struct" _ _ _) = mustChk (N.span n) "struct"
syn _ _ n = unexpectedNotation n "term in synthesizing position"

chk :: (ElabArgs) => SEnergy e -> Ctx -> TyV K -> Ntn -> IO (ElG e)
chk e c a n@(N.Block "sig" Nothing ns s) = case behavesAs a of
  Just (VU u) ->
    case e of
      SKinetic -> unsupportedInKineticMode (N.span n) "record type"
      SPotential -> do
        (xs, as) <- members c u ns
        let ty = Record (decodesInto u) xs as
        pure $ code $ G ty (eval c.elts ty)
  _ -> unexpectedTuple s $ prtVal c.shape a
chk e c a n@(N.Block "struct" Nothing ns s) = case behavesAs a of
  Just (VRecord _ xs te) -> do
    case e of
      SPotential -> unsupportedInPotentialMode (N.span n) "struct literal"
      SKinetic -> do
        unless (length xs == length ns) $
          failWith (N.span n) WrongNumberOfFields $
            "wrong number of fields, expected:"
              <+> pretty (length xs)
              <> ", but got:"
              <+> pretty (length ns)
        (ts, vs) <- elts c xs te ns
        pure $ G (Cons (Fields xs ts)) (VCons (Fields xs vs))
  _ -> unexpectedTuple s $ prtVal c.shape a
chk e c a n@(N.Infix n1 (N.Keyword "=>" _) n2) = case behavesAs a of
  Just (VPi _ dom cod) -> do
    x <- ident n1
    body <- withBound x dom c $ \v c' -> do
      g <- chk e c' (appClo cod v) n2
      pure g.stx
    pure $
      G
        (Lam (quote c.shape.length dom) (Abs x body))
        (VLam dom (Clo x (\v -> eval (c.elts :> v) body)))
  _ -> unexpectedLambda (N.span n) $ prtVal c.shape a
chk e c a n = do
  (g, a') <- syn e c n
  case (a, a') of
    (VU u, VU u') ->
      if leq (decodesInto u') (decodesInto u) then pure g else wrongLevel (N.span n)
    _ -> case defEq c.shape a a' of
      Left err ->
        conversionError (N.span n) (prtVal c.shape a) (prtVal c.shape a') err
      Right () -> pure g

-- Toplevel elaboration
--------------------------------------------------------------------------------

withArgs :: (ElabArgs) => Ctx -> [(Name, Ntn)] -> (Ctx -> IO (ElS e, TyS K)) -> IO (ElS e, TyS K)
withArgs c [] action = action c
withArgs c ((x, a_n) : args) action = do
  a <- typ c TheoryU a_n
  (t, b) <- withArgs (bind x a.val c) args action
  pure (Lam a.stx (Abs x t), Pi TheoryTop a.stx (Abs x b))

elabTheory :: (ElabArgs) => Ntn -> IO (Name, GlobalEntry)
elabTheory n = do
  (pat, body_n) <- definition n
  (name, args) <- unpackArgs pat
  (t, a) <- withArgs emptyCtx args $ \c -> do
    g <- chkP c (VU TheoryU) body_n
    pure (g.stx, U TheoryU)
  pure (name, PEntry t (eval mempty t) (eval mempty a))

elabDef :: (ElabArgs) => Ntn -> IO (Name, GlobalEntry)
elabDef n = do
  (head, body_n) <- definition n
  (pat, a_n) <- annot head
  (name, args) <- unpackArgs pat
  (t, a) <- withArgs emptyCtx args $ \c -> do
    ga <- typ c TheoryU a_n
    g <- chkK c ga.val body_n
    pure (g.stx, ga.stx)
  pure (name, KEntry t (eval mempty t) (eval mempty a))

elabDecl :: (ElabArgs) => Ntn -> IO (Name, GlobalEntry)
elabDecl (N.Decl "theory" n _) = elabTheory n
elabDecl (N.Decl "def" n _) = elabDef n
elabDecl n = unexpectedNotation n "top-level declaration"

elabTop :: ReporterFor ElaboratorCode -> File -> [Ntn] -> IO GlobalEnv
elabTop r f =
  let ?diagnosticCtx = DiagnosticCtx r f
      ?globalEnv = emptyGlobalEnv
   in go
  where
    go :: (ElabArgs) => [Ntn] -> IO GlobalEnv
    go [] = pure ?globalEnv
    go (n : ns) = do
      try (elabDecl n) >>= \case
        Right (x, entry) ->
          let ?globalEnv = insertEntry ?globalEnv x entry in go ns
        Left (_ :: ElabException) -> go ns
