module Geolog.Elaborator where

import Control.Exception
import Control.Monad (unless)
import Data.Kind (Type)
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
import Prelude hiding (head, init, lookup)

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
  | CantUseInductive
  | UseOfNonInductive
  deriving (Eq, Ord)

elaboratorCodeTable :: Map ElaboratorCode CodeMeta
elaboratorCodeTable =
  Map.fromList
    [ (FailedConversion, CodeMeta 0 SError Nothing)
    , (NotInScope, CodeMeta 1 SError Nothing)
    , (UnsupportedInPotentialMode, CodeMeta 2 SError Nothing)
    , (UnsupportedInKineticMode, CodeMeta 3 SError Nothing)
    , (ProjectionFromNonRecord, CodeMeta 4 SError Nothing)
    , (NoSuchField, CodeMeta 5 SError Nothing)
    , (ApplicationOfNonPi, CodeMeta 6 SError Nothing)
    , (MustChk, CodeMeta 7 SError Nothing)
    , (UnexpectedNotation, CodeMeta 8 SError Nothing)
    , (UnexpectedTuple, CodeMeta 9 SError Nothing)
    , (UnexpectedLambda, CodeMeta 10 SError Nothing)
    , (UnexpectedField, CodeMeta 11 SError Nothing)
    , (WrongNumberOfFields, CodeMeta 12 SError Nothing)
    , (WrongLevel, CodeMeta 13 SError Nothing)
    , (EqualityUnsupportedAtLevel, CodeMeta 14 SError Nothing)
    , (CantUseInductive, CodeMeta 15 SError Nothing)
    , (UseOfNonInductive, CodeMeta 16 SError Nothing)
    ]

-- Elaboration implicits
--------------------------------------------------------------------------------

data Ctx :: Type where
  CEmpty :: Ctx
  CBind :: ~Ctx -> BindingMode -> TyV K -> Ctx

swapBindingsTo :: BindingMode -> Ctx -> Ctx
swapBindingsTo _ CEmpty = CEmpty
swapBindingsTo bm (CBind c _ a) = CBind (swapBindingsTo bm c) bm a

lockCtx :: Ctx -> Ctx
lockCtx = swapBindingsTo BInductive

unlockCtx :: Ctx -> Ctx
unlockCtx = swapBindingsTo BConjunctive

data Scope = Scope
  { len :: Int
  , names :: Bwd Name
  , elts :: Bwd (ElV K)
  , isInductive :: Bool
  , ctx :: Ctx
  }

shape :: Scope -> CtxShape
shape sc = CtxShape sc.len sc.names

emptyScope :: Scope
emptyScope = Scope 0 mempty mempty False CEmpty

lock :: Scope -> Scope
lock sc = sc{isInductive = False, ctx = lockCtx sc.ctx}

unlock :: Scope -> Scope
unlock sc = sc{isInductive = True, ctx = unlockCtx sc.ctx}

unlockFor :: BindingMode -> Scope -> Scope
unlockFor BConjunctive = id
unlockFor BInductive = unlock

data DiagnosticCtx = DiagnosticCtx
  { reporter :: Reporter ElaboratorCode
  , file :: File
  }

type DiagnosticCtxArg = (?diagnosticCtx :: DiagnosticCtx)

type ElabArgs = (GlobalEnvArg, DiagnosticCtxArg)

bind :: Name -> TyV K -> Scope -> Scope
bind = bindAt BConjunctive

bindAt :: BindingMode -> Name -> TyV K -> Scope -> Scope
bindAt bm x a c =
  let v = local a (FId c.len)
   in letAt bm x v a c

withBound :: Name -> TyV K -> Scope -> (ElV K -> Scope -> a) -> a
withBound x a c action =
  let v = local a (FId c.len)
      c' = let_ x v a c
   in action v c'

letAt :: BindingMode -> Name -> ElV K -> TyV K -> Scope -> Scope
letAt bm x v a c =
  Scope
    { len = c.len + 1
    , names = c.names :> x
    , elts = c.elts :> v
    , isInductive = c.isInductive
    , ctx = CBind c.ctx bm a
    }

let_ :: Name -> ElV K -> TyV K -> Scope -> Scope
let_ = letAt BConjunctive

report :: (DiagnosticCtxArg) => Span -> ElaboratorCode -> DDoc -> IO ()
report s c m = do
  let n = Note (Just (SourceLoc ?diagnosticCtx.file s)) Nothing
  let d = Diagnostic c m [n]
  ?diagnosticCtx.reporter.reportIO d

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
  { stx :: (a e)
  , val :: (b e)
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
  use (G t v) = G (use t) (use v)
  init (G t v) = G (init t) (init v)

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
  ?diagnosticCtx.reporter.reportIO d
  evaluate $ throw GiveUp

wrongLevel :: (DiagnosticCtxArg) => Span -> IO a
wrongLevel s = failWith s WrongLevel "wrong level"

equalityUnsupportedAtLevel :: (DiagnosticCtxArg) => Span -> Level -> DDoc -> IO a
equalityUnsupportedAtLevel s l a =
  failWith s EqualityUnsupportedAtLevel $
    "equality types are unsupported at level" <+> dpretty l <> ", which is the inferred level of the type" <+> a

cantUseInductive :: (DiagnosticCtxArg) => Span -> Name -> IO a
cantUseInductive s x =
  failWith s CantUseInductive $ "variable" <+> dpretty x <+> "is bound inductively and thus cannot be used in a conjunctive context"

cantUseBuiltinInConjunctive :: (DiagnosticCtxArg) => Span -> Name -> IO a
cantUseBuiltinInConjunctive s x =
  failWith s CantUseInductive $ "builtin" <+> dpretty x <+> "cannot be used in a conjunctive context"

-- Helpers
--------------------------------------------------------------------------------

data VarError = VENotInScope | VEInductive

findLocal :: Scope -> Name -> Either VarError (ElG K, TyV K)
findLocal sc x = go sc.names sc.ctx sc.elts 0
 where
  go :: Bwd Name -> Ctx -> Bwd (ElV K) -> BId -> Either VarError (ElG K, TyV K)
  go (xs :> x') (CBind c bm a) (vs :> v) i
    | x == x' =
        if (bm == BInductive && sc.isInductive) || bm == BConjunctive
          then Right (G (LocalVar i) v, a)
          else Left VEInductive
    | otherwise = go xs c vs (i + 1)
  go _ _ _ _ = Left VENotInScope

findProj :: [Name] -> TeleV K -> [ElV K] -> Name -> Maybe (ElV K, TyV K)
findProj (x : xs) (TVCons a f) (v : vs) x'
  | x == x' = Just (v, a)
  | otherwise = findProj xs (f v) vs x'
findProj _ _ _ _ = Nothing

argBinding :: (DiagnosticCtxArg) => Ntn -> IO (Name, BindingMode, Ntn)
argBinding (N.Infix (N.Ident x _) (N.Keyword ":" _) n) = pure (x, BConjunctive, n)
argBinding (N.Infix (N.Ident x _) (N.Keyword "*:" _) n) = pure (x, BInductive, n)
argBinding n = unexpectedNotation n "arg binding"

binding :: (DiagnosticCtxArg) => Ntn -> IO (Name, Ntn)
binding (N.Infix (N.Ident x _) (N.Keyword ":" _) n) = pure (x, n)
binding n = unexpectedNotation n "binding"

annot :: (DiagnosticCtxArg) => Ntn -> IO (Ntn, Ntn)
annot (N.Infix n1 (N.Keyword ":" _) n2) = pure (n1, n2)
annot n = unexpectedNotation n "type annotation"

unpackArgs :: (DiagnosticCtxArg) => Ntn -> IO (Name, [(Name, BindingMode, Ntn)])
unpackArgs (N.App n ns) = do
  x <- ident n
  args <- mapM argBinding ns
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

members :: (ElabArgs) => Scope -> Universe -> [Ntn] -> IO ([Name], TeleS K)
members _ _ [] = pure ([], TSNil)
members sc u (n : ns) = do
  (x, n') <- binding n
  ga <- typ sc u n'
  (xs, as) <- members (bind x ga.val sc) u ns
  pure (x : xs, TSCons ga.stx as)

elts :: (ElabArgs) => Scope -> [Name] -> TeleV K -> [Ntn] -> IO ([ElS K], [ElV K])
elts _ [] TVNil [] = pure ([], [])
elts sc (x : xs) (TVCons a f) (n : ns) = do
  n' <- setting x n
  G t v <- chkK sc a n'
  (ts, vs) <- elts (let_ x v a sc) xs (f v) ns
  pure (t : ts, v : vs)
elts _ _ _ _ = panic "fail earlier if we don't have right number of fields"

typ :: (ElabArgs) => Scope -> Universe -> Ntn -> IO (TyG K)
typ sc u n = decode <$> chkK sc (VU u) n

synK :: (ElabArgs) => Scope -> Ntn -> IO (ElG K, TyV K)
synK = syn SKinetic

synP :: (ElabArgs) => Scope -> Ntn -> IO (ElG P, TyV K)
synP = syn SPotential

chkK :: (ElabArgs) => Scope -> TyV K -> Ntn -> IO (ElG K)
chkK = chk SKinetic

chkP :: (ElabArgs) => Scope -> TyV K -> Ntn -> IO (ElG P)
chkP = chk SPotential

guardDefEq :: (ElabArgs, DefEq a, Quote a b, DPrettyWithNames b) => Span -> Scope -> a -> a -> c -> IO c
guardDefEq s c v0 v1 x =
  case defEq (shape c) v0 v1 of
    Left err ->
      conversionError s (prtVal (shape c) v0) (prtVal (shape c) v1) err
    Right () -> pure x

-- -- syn and chk
-- --------------------------------------------------------------------------------

elim :: (ElabArgs) => Scope -> ElG K -> TyV K -> Ntn -> IO (ElG K, TyV K)
elim sc g a (N.Field "use" s) =
  case a of
    VInductive a' ->
      if sc.isInductive
        then pure (use g, a')
        else failWith s CantUseInductive "cannot .use an element of inductive type when in conjunctive mode"
    _ -> failWith s UseOfNonInductive "target of attempted .use is not an inductive type"
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
elim sc g a n =
  case behavesAs a of
    Just (VPi pv dom cod) -> do
      g' <- chkK (unlockFor (bindingMode pv) sc) dom n
      pure (app g g', appClo cod g'.val)
    _ ->
      failWith
        (N.span n)
        ApplicationOfNonPi
        "target of attempted application is not of a pi type"

syn :: (ElabArgs) => SEnergy e -> Scope -> Ntn -> IO (ElG e, TyV K)
syn SKinetic sc (N.Ident x s) = case findLocal sc x of
  Right res -> pure res
  Left VEInductive -> cantUseInductive s x
  Left VENotInScope -> case lookup ?globalEnv x of
    Just (KEntry _ v a) -> pure (G (GlobalVar x) v, a)
    Just (PEntry _ v a) -> pure (G (GlobalVar x) v', a)
     where
      v' = neu a (VGlobal x) SId (Just v)
    Nothing -> notInScope s x
syn SPotential _ (N.Ident _ s) = unsupportedInPotentialMode s "variables"
syn e c (N.App n []) = syn e c n
syn SKinetic c (N.App n0 (n1 : ns)) = do
  (g, a, elimNs) <- case n0 of
    N.Keyword "init" _ -> do
      ga <- typ (lock c) TheoryU n1
      pure (init ga, VInductive ga.val, ns)
    N.Keyword "pure" _ -> do
      (g, a) <- synK (unlock c) n1
      pure (G (Pure g.stx) (VPure g.val), VInductive a, ns)
    N.Keyword "Inductive" _ -> do
      ga <- typ (unlock c) TheoryU n1
      pure (code $ G (Inductive ga.stx) (VInductive ga.val), VU TheoryU, ns)
    _ -> do
      (g, a) <- synK c n0
      pure (g, a, n1 : ns)
  go g a elimNs
 where
  go g a [] = pure (g, a)
  go g a (n' : ns') = do
    (g', a') <- elim c g a n'
    go g' a' ns'
syn SPotential _ n@(N.App _ _) =
  unsupportedInPotentialMode (N.span n) "application"
syn SKinetic _ (N.Keyword "Set" _) =
  pure (code $ universe SetU, universe TheoryU)
syn SPotential _ (N.Keyword "Set" s) =
  unsupportedInPotentialMode s "universes"
syn SKinetic _ (N.Keyword "Prop" _) =
  pure (code $ universe SetU, universe TheoryU)
syn SPotential _ (N.Keyword "Prop" s) =
  unsupportedInPotentialMode s "universes"
syn SKinetic c (N.Infix n1 (N.Keyword arr@("*->"; "->") _) nb) =
  let bm = case arr of
        "*->" -> BInductive
        "->" -> BConjunctive
      pv = SetTheory bm
   in case n1 of
        (N.Infix (N.Ident x _) (N.Keyword ":" _) na) -> do
          ga <- typ (unlockFor bm c) SetU na
          gb <- typ (bindAt bm x ga.val c) TheoryU nb
          let t = Pi pv ga.stx (Abs x gb.stx)
          let v = VPi pv ga.val (Clo x (\w -> eval (c.elts :> w) gb.stx))
          pure (code $ G t v, universe TheoryU)
        na -> do
          ga <- typ (unlockFor bm c) SetU na
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
  unless (levelOf a == Set) $
    equalityUnsupportedAtLevel (N.span n) (levelOf a) (prtVal (shape c) a)
  pure (code $ G (Eq (quote c.len a) g0.stx g1.stx) (VEq a g0.val g1.val), universe SetU)
syn _ sc (N.Keyword "Int" s)
  | sc.isInductive = pure (code $ builtinTy BuiltinInt, universe SetU)
  | otherwise = cantUseBuiltinInConjunctive s "Int"
syn _ sc (N.Keyword "String" s)
  | sc.isInductive = pure (code $ builtinTy BuiltinString, universe SetU)
  | otherwise = cantUseBuiltinInConjunctive s "String"
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

chk :: (ElabArgs) => SEnergy e -> Scope -> TyV K -> Ntn -> IO (ElG e)
chk e c a n@(N.Block "sig" Nothing ns s) = case behavesAs a of
  Just (VU u) ->
    case e of
      SKinetic -> unsupportedInKineticMode (N.span n) "record type"
      SPotential -> do
        (xs, as) <- members c u ns
        let ty = Record (decodesInto u) xs as
        pure $ code $ G ty (eval c.elts ty)
  _ -> unexpectedTuple s $ prtVal (shape c) a
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
  _ -> unexpectedTuple s $ prtVal (shape c) a
chk e c a n@(N.Infix n1 (N.Keyword "=>" _) n2) = case behavesAs a of
  Just (VPi _ dom cod) -> do
    x <- ident n1
    body <- withBound x dom c $ \v c' -> do
      g <- chk e c' (appClo cod v) n2
      pure g.stx
    pure $
      G
        (Lam (quote c.len dom) (Abs x body))
        (VLam dom (Clo x (\v -> eval (c.elts :> v) body)))
  _ -> unexpectedLambda (N.span n) $ prtVal (shape c) a
chk e c a n = do
  (g, a') <- syn e c n
  case (a, a') of
    (VU u, VU u') ->
      if leq (decodesInto u') (decodesInto u) then pure g else wrongLevel (N.span n)
    _ -> case defEq (shape c) a a' of
      Left err ->
        conversionError (N.span n) (prtVal (shape c) a) (prtVal (shape c) a') err
      Right () -> pure g

-- Toplevel elaboration
--------------------------------------------------------------------------------

withArgs :: (ElabArgs) => Scope -> [(Name, BindingMode, Ntn)] -> (Scope -> IO (ElS e, TyS K)) -> IO (ElS e, TyS K)
withArgs c [] action = action c
withArgs c ((x, bm, a_n) : args) action = do
  a <- typ c TheoryU a_n
  (t, b) <- withArgs (bindAt bm x a.val c) args action
  pure (Lam a.stx (Abs x t), Pi (TheoryTop bm) a.stx (Abs x b))

elabType :: (ElabArgs) => Universe -> Ntn -> IO (Name, GlobalEntry)
elabType u n = do
  (pat, body_n) <- definition n
  (name, args) <- unpackArgs pat
  (t, a) <- withArgs emptyScope args $ \c -> do
    g <- chkP c (VU u) body_n
    pure (g.stx, U u)
  pure (name, PEntry t (eval mempty t) (eval mempty a))

elabDef :: (ElabArgs) => Ntn -> IO (Name, GlobalEntry)
elabDef n = do
  (head, body_n) <- definition n
  (pat, a_n) <- annot head
  (name, args) <- unpackArgs pat
  (t, a) <- withArgs emptyScope args $ \c -> do
    ga <- typ c TheoryU a_n
    g <- chkK c ga.val body_n
    pure (g.stx, ga.stx)
  pure (name, KEntry t (eval mempty t) (eval mempty a))

elabDecl :: (ElabArgs) => Ntn -> IO (Name, GlobalEntry)
elabDecl (N.Decl "theory" n _) = elabType TheoryU n
elabDecl (N.Decl "set" n _) = elabType SetU n
elabDecl (N.Decl "def" n _) = elabDef n
elabDecl n = unexpectedNotation n "top-level declaration"

elabTop :: Reporter ElaboratorCode -> File -> [Ntn] -> IO GlobalEnv
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
