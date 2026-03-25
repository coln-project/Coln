module Geolog.Elaborator where

import Control.Exception
import Control.Monad (unless)
import Data.Map (Map)
import Data.Map qualified as Map
import Geolog.Common
import Geolog.Core
import Geolog.CoreOperations
import Diagnostician
import FNotation (Ntn, Name)
import FNotation qualified as N
import Geolog.Pretty
import Prettyprinter
import Prelude hiding (lookup)

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
  deriving (Eq, Ord)

elaboratorCodeTable :: Map ElaboratorCode CodeMeta
elaboratorCodeTable = Map.fromList
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
    (WrongLevel, CodeMeta 13 SError Nothing)
  ]

-- Elaboration implicits
--------------------------------------------------------------------------------

type Ctx = Bwd (TyV K)

type CtxArg = (?ctx :: Ctx)

data DiagnosticCtx = DiagnosticCtx
  { reporter :: ReporterFor ElaboratorCode,
    file :: File
  }

type DiagnosticCtxArg = (?diagnosticCtx :: DiagnosticCtx)

type Elab a = (CtxArg, CtxLenArg, NamesArg, EnvArg, GlobalEnvArg, DiagnosticCtxArg) => a

bind :: Elab (Name -> TyV K -> Elab (ElV K -> a) -> a)
bind x a action =
  let i = FId ?ctxLen
      v = local a i
   in let_ x v a action

let_ :: Elab (Name -> ElV K -> TyV K -> Elab (ElV K -> a) -> a)
let_ x v a action =
  let ?ctx = ?ctx :> a
      ?ctxLen = ?ctxLen + 1
      ?names = ?names :> x
      ?env = ?env :> v
   in action v

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
  decode u (G t v) = G (decode u t) (decode u v)
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

-- Helpers
--------------------------------------------------------------------------------

findLocal :: Elab (Name -> Maybe (ElG K, TyV K))
findLocal x = go ?names ?ctx ?env 0
  where
    go :: Bwd Name -> Bwd (TyV K) -> Bwd (ElV K) -> BId -> Maybe (ElG K, TyV K)
    go (xs :> x') (as :> a) (vs :> v) i
      | x == x' = Just (G (Var i) v, a)
      | otherwise = go xs as vs (i + 1)
    go _ _ _ _ = Nothing

findProj :: Elab ([Name] -> TeleV (TyV K) -> [ElV K] -> Name -> Maybe (ElV K, TyV K))
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

members :: Elab (Universe -> [Ntn] -> IO ([Name], [TyS K]))
members _ [] = pure ([], [])
members u (n : ns) = do
  (x, n') <- binding n
  ga <- typ u n'
  (xs, as) <- bind x ga.val $ \_ -> members u ns
  pure (x : xs, ga.stx : as)

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

elts :: Elab ([Name] -> TeleV (TyV K) -> [Ntn] -> IO ([ElS K], [ElV K]))
elts [] TVNil [] = pure ([], [])
elts (x : xs) (TVCons a f) (n : ns) = do
  n' <- setting x n
  G t v <- chkK a n'
  (ts, vs) <- let_ x v a $ \_ -> elts xs (f v) ns
  pure (t : ts, v : vs)
elts _ _ _ = panic "fail earlier if we don't have right number of fields"

typ :: Elab (Universe -> Ntn -> IO (TyG K))
typ u n = decode u <$> chkK (VU u) n

synK :: Elab (Ntn -> IO (ElG K, TyV K))
synK = syn SKinetic

synP :: Elab (Ntn -> IO (ElG P, TyV K))
synP = syn SPotential

chkK :: Elab (TyV K -> Ntn -> IO (ElG K))
chkK = chk SKinetic

chkP :: Elab (TyV K -> Ntn -> IO (ElG P))
chkP = chk SPotential

-- syn and chk
--------------------------------------------------------------------------------

elim :: Elab (ElG K -> TyV K -> Ntn -> IO (ElG K, TyV K))
elim g a (N.Field x s) = 
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
elim g a n = 
  case behavesAs a of
    Just (VPi _ dom cod) -> do
      g' <- chkK dom n
      pure (app g g', appClo cod g'.val)
    _ ->
      failWith
        (N.span n)
        ApplicationOfNonPi
        "target of attempted application is not of a pi type"

syn :: Elab (SEnergy e -> Ntn -> IO (ElG e, TyV K))
syn SKinetic (N.Ident x s) = case findLocal x of
  Just res -> pure res
  Nothing -> case lookup ?globalEnv x of
    Just (KEntry _ v a) -> pure (G (GlobalVar x) v, a)
    Just (PEntry _ v a) -> pure (G (GlobalVar x) v', a)
      where
        v' = neu a (Global x) SId (Just v)
    Nothing -> notInScope s x
syn SPotential (N.Ident _ s) = unsupportedInPotentialMode s "variables"
syn SKinetic (N.App n ns) = do
  (g, a) <- synK n
  go g a ns
  where
    go g a [] = pure (g, a)
    go g a (n:ns) = do
      (g', a') <- elim g a n
      go g' a' ns
syn SPotential n@(N.App _ _) =
  unsupportedInPotentialMode (N.span n) "application"
syn SKinetic (N.Keyword "Query" _) =
  pure (code $ universe QueryU, universe TheoryU)
syn SPotential (N.Keyword "Query" s) =
  unsupportedInPotentialMode s "universes"
syn SKinetic (N.Infix n1 (N.Keyword arr@("~>"; "->") _) nb) =
  let (domU, pv) = case arr of
        "~>" -> (PrimU, PrimTheory)
        "->" -> (QueryU, QueryTheory)
   in case n1 of
        (N.Infix (N.Ident x _) (N.Keyword ":" _) na) -> do
          ga <- typ domU na
          gb <- bind x ga.val $ \_ -> typ TheoryU nb
          let t = Pi pv ga.stx (Abs x gb.stx)
          let v = VPi pv ga.val (Clo x (\w -> evalIn (?env :> w) gb.stx))
          pure (code $ G t v, universe TheoryU)
        na -> do
          ga <- typ domU na
          gb <- typ TheoryU nb
          let t = Pi pv ga.stx (AbsConst gb.stx)
          let v = VPi pv ga.val (CloConst gb.val)
          pure (code $ G t v, universe TheoryU)
syn SPotential n@(N.Infix _ (N.Keyword "->" _) _) =
  unsupportedInPotentialMode (N.span n) "pi types"
syn _ (N.Keyword "Int" _) = pure (code $ builtinTy BuiltinInt, universe PrimU)
syn _ (N.Keyword "String" _) = pure (code $ builtinTy BuiltinString, universe PrimU)
syn SKinetic (N.String s _) = pure (lit $ LitString s, builtinTy BuiltinString)
syn SPotential (N.String _ sp) =
  unsupportedInPotentialMode sp "string literals"
syn SKinetic (N.Int i _) = pure (lit $ LitInt i, builtinTy BuiltinInt)
syn SPotential (N.Int _ sp) =
  unsupportedInPotentialMode sp "int literals"
syn _ n@(N.Infix _ (N.Keyword "=>" _) _) = mustChk (N.span n) "lambda syntax"
syn _ n@(N.Tuple _ _) = mustChk (N.span n) "tuple syntax"
syn _ n = unexpectedNotation n "term in synthesizing position"

chk :: Elab (SEnergy e -> TyV K -> Ntn -> IO (ElG e))
chk e a n@(N.Tuple ns s) = case behavesAs a of
  Just (VU u) ->
    case e of
      SKinetic -> unsupportedInKineticMode (N.span n) "record type"
      SPotential -> do
        (xs, as) <- members u ns
        let ty = Record (decodesInto u) xs as
        pure $ code $ G ty (eval ty)
  Just (VRecord _ xs te) -> do
    case e of
      SPotential -> unsupportedInPotentialMode (N.span n) "tuple literal"
      SKinetic -> do
        unless (length xs == length ns) $
          failWith (N.span n) WrongNumberOfFields $
            "wrong number of fields, expected:"
              <+> pretty (length xs)
              <> ", but got:"
              <+> pretty (length ns)
        (ts, vs) <- elts xs te ns
        pure $ G (Cons (Fields xs ts)) (VCons (Fields xs vs))
  _ -> unexpectedTuple s $ prtTop $ quote a
chk e a n@(N.Infix n1 (N.Keyword "=>" _) n2) = case behavesAs a of
  Just (VPi _ dom cod) -> do
    x <- ident n1
    body <- bind x dom $ \v -> do
      g <- chk e (appClo cod v) n2
      pure g.stx
    pure $
      G
        (Lam (quote dom) (Abs x body))
        (VLam dom (Clo x (\v -> evalIn (?env :> v) body)))
  _ -> unexpectedLambda (N.span n) $ prtTop $ quote a
chk e a n = do
  (g, a') <- syn e n
  case (a, a') of
    (VU u, VU u') ->
      if leq (decodesInto u') (decodesInto u) then pure g else wrongLevel (N.span n)
    _ -> case defEq a a' of
      Left err ->
        conversionError (N.span n) (prtTop $ quote a) (prtTop $ quote a') err
      Right () -> pure g

-- Toplevel elaboration
--------------------------------------------------------------------------------

withArgs :: Elab ([(Name, Ntn)] -> Elab (IO (ElS e, TyS K)) -> IO (ElS e, TyS K))
withArgs [] action = action
withArgs ((x, a_n) : args) action = do
  a <- typ TheoryU a_n
  bind x a.val $ \_ -> do
    (t, b) <- withArgs args action
    pure (Lam a.stx (Abs x t), Pi TheoryTop a.stx (Abs x b))

elabTheory :: Elab (Ntn -> IO (Name, GlobalEntry))
elabTheory n = do
  (pat, body_n) <- definition n
  (name, args) <- unpackArgs pat
  (t, a) <- withArgs args $ do
    g <- chkP (VU TheoryU) body_n
    pure (g.stx, U TheoryU)
  pure (name, PEntry t (eval t) (eval a))

elabDef :: Elab (Ntn -> IO (Name, GlobalEntry))
elabDef n = do
  (head, body_n) <- definition n
  (pat, a_n) <- annot head
  (name, args) <- unpackArgs pat
  (t, a) <- withArgs args $ do
    ga <- typ TheoryU a_n
    g <- chkK ga.val body_n
    pure (g.stx, ga.stx)
  pure (name, KEntry t (eval t) (eval a))

elabDecl :: Elab (Ntn -> IO (Name, GlobalEntry))
elabDecl (N.Decl "theory" n _) = elabTheory n
elabDecl (N.Decl "def" n _) = elabDef n
elabDecl n = unexpectedNotation n "top-level declaration"

elabTop :: ReporterFor ElaboratorCode -> File -> [Ntn] -> IO GlobalEnv
elabTop r f =
  let ?env = BwdNil
      ?diagnosticCtx = DiagnosticCtx r f
      ?ctx = BwdNil
      ?ctxLen = 0
      ?names = BwdNil
      ?globalEnv = emptyGlobalEnv
   in go
  where
    go :: Elab ([Ntn] -> IO GlobalEnv)
    go [] = pure ?globalEnv
    go (n : ns) = do
      try (elabDecl n) >>= \case
        Right (x, entry) ->
          let ?globalEnv = insertEntry ?globalEnv x entry in go ns
        Left (_ :: ElabException) -> go ns
