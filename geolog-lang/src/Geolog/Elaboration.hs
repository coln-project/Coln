module Geolog.Elaboration where

import Control.Exception
import Control.Monad (unless)
import Geolog.Common
import Geolog.Core
import Geolog.Diagnostics
import Geolog.Diagnostics.Code qualified as C
import Geolog.Evaluation
import Geolog.Notation (Ntn)
import Geolog.Notation qualified as N
import Geolog.Pretty hiding (bind)
import Prettyprinter
import Prelude hiding (lookup)

-- Elaboration context
--------------------------------------------------------------------------------

newtype Ctx = Ctx {elts :: Bwd (QName, TyV)}

instance Lookup Ctx QName (BId, TyV) where
  lookup c x = go c.elts 0
    where
      go BwdNil _ = Nothing
      go (es :> (x', va)) i
        | x == x' = Just (i, va)
        | otherwise = go es (i + 1)

type CtxArg = (?ctx :: Ctx)

data DiagCtx = DiagCtx {reporter :: Reporter, file :: File}

type DiagCtxArg = (?diagCtx :: DiagCtx)

data ElabException = GiveUp
  deriving (Show)

instance Exception ElabException

report :: (DiagCtxArg) => Span -> C.Code -> IO a
report s c = do
  let n = Note (Just (SourceLoc ?diagCtx.file s)) Nothing
  let d = Diagnostic c [n]
  reportIO ?diagCtx.reporter d
  evaluate $ throw GiveUp

type Elab a = (DiagCtxArg, CtxArg, CtxLenArg, EnvArg) => a

withNames :: Elab (((NamesArg) => a) -> a)
withNames f = let ?names = fst <$> ?ctx.elts in f

pp :: (Prt a) => Elab (a -> Doc ann)
pp x = withNames $ prtPrec precTop x

-- Context manipulation
--------------------------------------------------------------------------------

bind :: Elab (QName -> TyV -> (Elab a) -> a)
bind x va f = let vx = VNeu fresh SId in let_ x vx va f

bindVal :: Elab (QName -> TyV -> (Elab (ElV -> a)) -> a)
bindVal x va f = let vx = VNeu fresh SId in let_ x vx va (f vx)

let_ :: Elab (QName -> ElV -> TyV -> (Elab a) -> a)
let_ x vx va f =
  let ?env = ?env :> vx
      ?ctx = Ctx $ ?ctx.elts :> (x, va)
      ?ctxLen = ?ctxLen + 1
   in f

-- Pattern matching
--------------------------------------------------------------------------------

binding :: (DiagCtxArg) => Ntn -> IO (QName, Ntn)
binding (N.Infix (N.Ident x _) (N.Keyword ":" _) n) = pure (x, n)
binding n = report (N.span n) (C.Expected C.Binding)

annot :: (DiagCtxArg) => Ntn -> IO (Ntn, Ntn)
annot (N.Infix n1 (N.Keyword ":" _) n2) = pure (n1, n2)
annot n = report (N.span n) (C.Expected C.Annot)

setting :: (DiagCtxArg) => QName -> Ntn -> IO Ntn
setting x (N.Infix (N.Field x' sp) (N.Keyword "=" _) n')
  | x == x' = pure n'
  | otherwise = report sp (C.ExpectedField x x')
setting _ n = report (N.span n) (C.UnexpectedNotation "record entry")

ident :: (DiagCtxArg) => Ntn -> IO QName
ident (N.Ident x _) = pure x
ident n = report (N.span n) (C.UnexpectedNotation "ident")

-- Glued core
--------------------------------------------------------------------------------

data Glued s v = G {stx :: s, val :: v}

type ElG = Glued ElS ElV

type TyG = Glued TyS TyV

instance Core ElG TyG where
  app (G t1 v1) (G t2 v2) = G (app t1 t2) (app v1 v2)
  proj (G t v) x = G (proj t x) (proj v x)
  code (G t v) = G (code t) (code v)
  decode u (G t v) = G (decode u t) (decode u v)
  universe u = G (universe u) (universe u)

instance LevelOf TyG where
  levelOf (G _ va) = levelOf va

-- Type elaboration
--------------------------------------------------------------------------------

members :: Elab (Level -> [Ntn] -> IO [(QName, TyS)])
members _ [] = pure []
members l (n : ns) = do
  (x, n') <- binding n
  G sa va <- typChk l n'
  ((x, sa) :) <$> bind x va (members l ns)

typChk :: Elab (Level -> Ntn -> IO TyG)
typChk l = \case
  N.Tuple ns _ -> do
    fs <- Fields <$> members l ns
    pure $ G (Record l fs) (VRecord l ?env fs)
  n -> do
    a <- typSyn n
    if leq (levelOf a) l
      then pure a
      else report (N.span n) (C.OutOfUniverse (levelOf a) l)

typSyn :: Elab (Ntn -> IO TyG)
typSyn n = do
  (g, a) <- syn n
  case a of
    VU u -> pure $ decode u g
    _ -> report (N.span n) C.SynthesizedNonUniverse

-- Synthesis
--------------------------------------------------------------------------------

syn :: Elab (Ntn -> IO (ElG, TyV))
syn n = case n of
  N.Ident x sp -> case lookup ?ctx x of
    Just (i, va) -> pure (G (Var i) (elemAt ?env i), va)
    Nothing -> report sp (C.NotInScope x)
  N.App n1 (N.Field x sp) -> do
    (gr, va) <- syn n1
    case va of
      VRecord _ env (Fields ms) -> go env ms
        where
          go _ [] = report sp (C.NoSuchField x)
          go e ((x', a) : ms')
            | x == x' = pure (proj gr x, evalIn e a)
            | otherwise = go (e :> (proj gr.val x')) ms'
      _ -> report (N.span n1) C.CannotProjectNonRecord
  N.App n1 n2 -> do
    (gf, vab) <- syn n1
    case vab of
      VPi _ va b -> do
        gt <- chk va n2
        pure (app gf gt, appClo b gt.val)
      _ -> report (N.span n1) C.CannotApplyNonPi
  N.Keyword "Query" _ -> pure (code $ universe QueryU, universe TheoryU)
  N.Infix n1 (N.Keyword "->" _) n2 -> do
    (x, na) <- binding n1
    a <- typSyn na
    b <- bind x a.val $ typSyn n2
    let pv = piVariant (levelOf a) (levelOf b)
    case universeFor (levelOf pv) of
      Just u ->
        pure
          ( code $ G (Pi pv a.stx (Abs x b.stx)) (VPi pv a.val (Clo ?env x b.stx)),
            VU u
          )
      Nothing -> report (N.span n) (C.NoUniverseForPi pv)
  N.Infix _ (N.Keyword "=>" _) _ -> report (N.span n) (C.MustChk "lambda syntax")
  N.Tuple _ _ -> report (N.span n) (C.MustChk "tuple syntax")
  _ -> report (N.span n) (C.UnexpectedNotation "term")

-- Checking
--------------------------------------------------------------------------------

elts ::
  Elab
    ( Env ->
      [(QName, TyS)] ->
      [Ntn] ->
      IO ([(QName, ElS)], [(QName, ElV)])
    )
elts _ [] [] = pure ([], [])
elts e ((x, a) : ms) (n : ns) = do
  n' <- setting x n
  let va = evalIn e a
  G st vt <- chk va n'
  (sfs, vfs) <- let_ x vt va $ elts (e :> vt) ms ns
  pure ((x, st) : sfs, (x, vt) : vfs)
elts _ _ _ = impossible

chk :: Elab (TyV -> Ntn -> IO ElG)
chk va n = case va of
  VU u -> do
    g <- typChk (decodesInto u) n
    pure $ code g
  _ -> case n of
    N.Tuple ns _ -> case va of
      VRecord _ env (Fields ms) -> do
        unless (length ms == length ns) $ do
          report (N.span n) (C.WrongNumberOfFields (length ms) (length ns))
        (sfs, vfs) <- elts env ms ns
        pure $ G (Cons (Fields sfs)) (VCons (Fields vfs))
      _ -> report (N.span n) (C.TupleFoundAtUnexpectedType $ pp $ quote va)
    N.Infix n1 (N.Keyword "=>" _) n2 -> case va of
      VPi _ vdom (Clo env _ cod) -> do
        x <- ident n1
        body <- bindVal x vdom $ \vx -> do
          let vcod = evalIn (env :> vx) cod
          G body _ <- chk vcod n2
          pure body
        pure $ G (Lam (Abs x body)) (VLam (Clo ?env x body))
      _ -> report (N.span n) (C.UnexpectedNotation "non-pi type")
    _ -> do
      (g, va') <- syn n
      let sp = N.span n
      withNames $ case isConv va va' of
        Success () -> pure g
        Failure (NotConvertableEl d d') r -> report sp (C.NotConvertableEl d d' r)
        Failure (NotConvertableTy d d') r -> report sp (C.NotConvertableTy d d' r)

definition :: Elab (Ntn -> IO (Ntn, Ntn))
definition (N.Infix n1 (N.Keyword "=" _) n2) = pure (n1, n2)
definition n = report (N.span n) (C.Expected C.Definition)

unpackArgs :: Elab (Ntn -> IO (QName, [(QName, Ntn)]))
unpackArgs n = go n []
  where
    go (N.Ident x _) args = pure (x, args)
    go (N.App n1 n2) args = do
      b <- binding n2
      go n1 $ b : args
    go _ _ = report (N.span n) (C.Expected C.ApplicationPattern)

elabTheory :: Elab (Ntn -> IO (QName, ElG, TyV))
elabTheory n = do
  (headN, bodyN) <- definition n
  (x, argsN) <- unpackArgs headN
  (args, body) <- go argsN bodyN
  let ty = wrapPis args
  let el = wrapLams args body
  pure $ (x, G el (eval el), eval ty)
  where
    wrapPis :: [(QName, TyS)] -> TyS
    wrapPis [] = U TheoryU
    wrapPis ((x, a) : rest) = Pi TopTop a (Abs x (wrapPis rest))
    wrapLams :: [(QName, TyS)] -> TyS -> ElS
    wrapLams [] body = Code body
    wrapLams ((x, _) : rest) body = Lam (Abs x (wrapLams rest body))
    go :: Elab ([(QName, Ntn)] -> Ntn -> IO ([(QName, TyS)], TyS))
    go [] bodyN = do
      G body _ <- typChk Theory bodyN
      pure ([], body)
    go ((x, tyN) : argsN) bodyN = do
      G a va <- typChk Theory tyN
      (args, body) <- bind x va $ go argsN bodyN
      pure ((x, a) : args, body)

elabDef :: Elab (Ntn -> IO (QName, ElG, TyV))
elabDef n = do
  (headN, bodyN) <- definition n
  (pat, tyN) <- annot headN
  (x, argsN) <- unpackArgs pat
  (args, retTy, body) <- go argsN tyN bodyN
  let ty = wrapPis args retTy
  let el = wrapLams args body
  pure $ (x, G el (eval el), eval ty)
  where
    go :: Elab ([(QName, Ntn)] -> Ntn -> Ntn -> IO ([(QName, TyS)], TyS, ElS))
    go [] tyN bodyN = do
      G a va <- typChk Theory tyN
      G t _ <- chk va bodyN
      pure ([], a, t)
    go ((x, argTyN) : argsN) tyN bodyN = do
      G a va <- typChk Theory argTyN
      (args, ty, body) <- bind x va $ go argsN tyN bodyN
      pure ((x, a) : args, ty, body)
    wrapPis :: [(QName, TyS)] -> TyS -> TyS
    wrapPis [] ty = ty
    wrapPis ((x, a) : args) ty =
      Pi TopTop a (Abs x (wrapPis args ty))
    wrapLams :: [(QName, TyS)] -> ElS -> ElS
    wrapLams [] body = body
    wrapLams ((x, _) : args) body = Lam (Abs x (wrapLams args body))

elabDecl :: Elab (Ntn -> IO (QName, ElG, TyV))
elabDecl (N.Decl "theory" n _) = elabTheory n
elabDecl (N.Decl "def" n _) = elabDef n
elabDecl n = report (N.span n) (C.Expected C.Declaration)

elabTop :: Reporter -> File -> [Ntn] -> IO [(QName, ElS, TyS)]
elabTop r f =
  let ?env = BwdNil
      ?diagCtx = DiagCtx r f
      ?ctx = Ctx BwdNil
      ?ctxLen = 0
   in go BwdNil
  where
    go :: Elab (Bwd (QName, ElS, TyS) -> [Ntn] -> IO [(QName, ElS, TyS)])
    go ds [] = pure $ toList ds
    go ds (n : ns) = do
      try (elabDecl n) >>= \case
        Right (x, G t v, va) -> do
          let a = quote va
          let_ x v va $ go (ds :> (x, t, a)) ns
        Left (_ :: ElabException) -> go ds ns
