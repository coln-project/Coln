module Geolog.Elaborator where

import Control.Exception
import Control.Monad (unless)
import Geolog.Common
import Geolog.Core
import Geolog.CoreOperations
import Geolog.Diagnostician
import Geolog.Elaborator.Diagnostics
import Geolog.Notation (Ntn)
import Geolog.Notation qualified as N
import Geolog.Pretty
import Prettyprinter
import Prelude hiding (lookup)

type Ctx = Bwd (TyV K)

type CtxArg = (?ctx :: Ctx)

data DiagnosticCtx = DiagnosticCtx
  { reporter :: Reporter
  , file :: File
  }

type DiagnosticCtxArg = (?diagnosticCtx :: DiagnosticCtx)

type Elab a = (CtxArg, CtxLenArg, NamesArg, EnvArg, GlobalEnvArg, DiagnosticCtxArg) => a

bind :: Elab (QName -> TyV K -> Elab (ElV K -> a) -> a)
bind x a action =
  let i = FId ?ctxLen
      v = local a i
   in let_ x v a action

let_ :: Elab (QName -> ElV K -> TyV K -> Elab (ElV K -> a) -> a)
let_ x v a action =
  let
    ?ctx = ?ctx :> a
    ?ctxLen = ?ctxLen + 1
    ?names = ?names :> x
    ?env = ?env :> v
   in
    action v

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
  decode u (G t v) = G (decode u t) (decode u v)
  universe u = G (universe u) (universe u)

report :: (DiagnosticCtxArg) => Span -> ElaboratorCode -> ADoc -> IO ()
report s c m = do
  let n = Note (Just (SourceLoc ?diagnosticCtx.file s)) Nothing
  let d = Diagnostic (ElaboratorCode c) m [n]
  reportIO ?diagnosticCtx.reporter d

data ElabException = GiveUp
  deriving (Show)

instance Exception ElabException

failWith :: (DiagnosticCtxArg) => Span -> ElaboratorCode -> ADoc -> IO a
failWith s c m = do
  report s c m
  evaluate $ throw GiveUp

typ :: Elab (Universe -> Ntn -> IO (TyG K))
typ u n = decode u <$> chkK (VU u) n

findLocal :: Elab (QName -> Maybe (ElG K, TyV K))
findLocal x = go ?names ?ctx ?env 0
 where
  go :: Bwd QName -> Bwd (TyV K) -> Bwd (ElV K) -> BId -> Maybe (ElG K, TyV K)
  go (xs :> x') (as :> a) (vs :> v) i
    | x == x' = Just (G (Var i) v, a)
    | otherwise = go xs as vs (i + 1)
  go _ _ _ _ = Nothing

findProj :: Elab ([QName] -> TeleV (TyV K) -> [ElV K] -> QName -> Maybe (ElV K, TyV K))
findProj (x : xs) (TVCons a f) (v : vs) x'
  | x == x' = Just (v, a)
  | otherwise = findProj xs (f v) vs x'
findProj _ _ _ _ = Nothing

notInScope :: (DiagnosticCtxArg) => Span -> QName -> IO a
notInScope s x = failWith s NotInScope $ "identifier" <+> pretty x <+> "not in scope"

unsupportedInPotentialMode :: (DiagnosticCtxArg) => Span -> ADoc -> IO a
unsupportedInPotentialMode s feature =
  failWith s UnsupportedInPotentialMode $
    feature <+> "unsupported while elaborating a potential term"

unsupportedInKineticMode :: (DiagnosticCtxArg) => Span -> ADoc -> IO a
unsupportedInKineticMode s feature =
  failWith s UnsupportedInKineticMode $
    feature <+> "unsupported while elaborating a kinetic term"

mustChk :: (DiagnosticCtxArg) => Span -> ADoc -> IO a
mustChk s feature =
  failWith s MustChk $
    feature <+> "unsupported while in synthesis mode"

unexpectedNotation :: (DiagnosticCtxArg) => Ntn -> ADoc -> IO a
unexpectedNotation n c =
  failWith (N.span n) UnexpectedNotation $
    "unexpected notation for" <+> c <> ":" <+> N.head n

syn :: Elab (SEnergy e -> Ntn -> IO (ElG e, TyV K))
syn SKinetic (N.Ident x s) = case findLocal x of
  Just res -> pure res
  Nothing ->
    let c = Constant x
     in case lookup ?globalEnv c of
          Just (KEntry _ v a) -> pure (G (GlobalVar c) v, a)
          Just (PEntry _ v a) -> pure (G (GlobalVar c) (VNeu n), a)
           where
            n = Neutral (Global c) SId (BehavesAs v) a
          Nothing -> notInScope s x
syn SPotential (N.Ident _ s) = unsupportedInPotentialMode s "variables"
syn SKinetic (N.App n (N.Field x s)) = do
  (g, a) <- synK n
  case behavesAs a of
    Just (VRecord _ xs te) -> case findProj xs te (coerceToFields g.val).values x of
      Just (v, a') -> pure (G (Proj g.stx x) v, a')
      Nothing ->
        failWith s NoSuchField $
          "no such field:" <+> pretty x
    _ ->
      failWith
        (N.span n)
        ProjectionFromNonRecord
        "target of attempted field projection is not of a record type"
syn SKinetic (N.App n1 n2) = do
  (g1, a1) <- synK n1
  case behavesAs a1 of
    Just (VPi _ a b) -> do
      g2 <- chkK a n2
      pure (app g1 g2, appClo b g2.val)
    _ ->
      failWith
        (N.span n1)
        ApplicationOfNonPi
        "target of attempted application is not of a pi type"
syn SPotential n@(N.App _ _) =
  unsupportedInPotentialMode (N.span n) "application"
syn SKinetic (N.Keyword "Query" _) =
  pure (code $ universe QueryU, universe TheoryU)
syn SPotential (N.Keyword "Query" s) =
  unsupportedInPotentialMode s "universes"
syn SKinetic (N.Infix n1 (N.Keyword "->" _) nb) = case n1 of
  (N.Infix (N.Ident x _) (N.Keyword ":" _) na) -> do
    ga <- typ QueryU na
    gb <- bind x ga.val $ \_ -> typ TheoryU nb
    let pv = QueryTheory
    let t = Pi pv ga.stx (Abs x gb.stx)
    let v = VPi pv ga.val (Clo x (\w -> evalIn (?env :> w) gb.stx))
    pure (code $ G t v, universe TheoryU)
  na -> do
    ga <- typ QueryU na
    gb <- typ TheoryU nb
    let pv = QueryTheory
    let t = Pi pv ga.stx (AbsConst gb.stx)
    let v = VPi pv ga.val (CloConst gb.val)
    pure (code $ G t v, universe TheoryU)
syn SPotential n@(N.Infix _ (N.Keyword "->" _) _) =
  unsupportedInPotentialMode (N.span n) "pi types"
syn _ n@(N.Infix _ (N.Keyword "=>" _) _) = mustChk (N.span n) "lambda syntax"
syn _ n@(N.Tuple _ _) = mustChk (N.span n) "tuple syntax"
syn _ n = unexpectedNotation n "element"

synK :: Elab (Ntn -> IO (ElG K, TyV K))
synK = syn SKinetic

synP :: Elab (Ntn -> IO (ElG P, TyV K))
synP = syn SPotential

unexpectedTuple :: (DiagnosticCtxArg) => Span -> ADoc -> IO a
unexpectedTuple s a =
  failWith s UnexpectedTuple $
    "tried to check tuple notation at type" <+> a <+> "which is not a record type"

unexpectedLambda :: (DiagnosticCtxArg) => Span -> ADoc -> IO a
unexpectedLambda s a =
  failWith s UnexpectedLambda $
    "tried to check lambda notation at type" <+> a <+> "which is not a pi type"

conversionError :: (DiagnosticCtxArg) => Span -> ADoc -> ADoc -> DefEqCheckError -> IO a
conversionError = unimplemented

binding :: (DiagnosticCtxArg) => Ntn -> IO (QName, Ntn)
binding (N.Infix (N.Ident x _) (N.Keyword ":" _) n) = pure (x, n)
binding n = unexpectedNotation n "binding"

members :: Elab (Universe -> [Ntn] -> IO ([QName], [TyS K]))
members _ [] = pure ([], [])
members u (n : ns) = do
  (x, n') <- binding n
  ga <- typ u n'
  (xs, as) <- bind x ga.val $ \_ -> members u ns
  pure (x : xs, ga.stx : as)

setting :: (DiagnosticCtxArg) => QName -> Ntn -> IO Ntn
setting x (N.Infix (N.Field x' sp) (N.Keyword "=" _) n')
  | x == x' = pure n'
  | otherwise =
      failWith sp UnexpectedField $
        "got field" <+> pretty x' <> ", expected field" <+> pretty x
setting _ n = unexpectedNotation n "record field"

ident :: (DiagnosticCtxArg) => Ntn -> IO QName
ident (N.Ident x _) = pure x
ident n = unexpectedNotation n "ident"

elts :: Elab ([QName] -> TeleV (TyV K) -> [Ntn] -> IO ([ElS K], [ElV K]))
elts [] TVNil [] = pure ([], [])
elts (x : xs) (TVCons a f) (n : ns) = do
  n' <- setting x n
  G t v <- chkK a n'
  (ts, vs) <- let_ x v a $ \_ -> elts xs (f v) ns
  pure (t : ts, v : vs)
elts _ _ _ = panic "fail earlier if we don't have right number of fields"

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
  case defEq a a' of
    Left err ->
      conversionError (N.span n) (prtTop $ quote a) (prtTop $ quote a') err
    Right () -> pure g

chkK :: Elab (TyV K -> Ntn -> IO (ElG K))
chkK = chk SKinetic

chkP :: Elab (TyV K -> Ntn -> IO (ElG P))
chkP = chk SPotential
