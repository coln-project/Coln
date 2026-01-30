{- |
We have three things to deal with:

1. Local context
2. Global context
3. Scope

Eventually, when we deal with namespaces properly, we will reimplement
something like yuujinchou.

Until then, we can make do with just local context.

We are also going to report at most one error for each top-level binding.
When we implement unification, we can revisit this.
-}
module Geolog.Elaboration where

import Control.Exception
import Control.Monad (unless)
import Data.Singletons
import Data.Text (Text)
import Geolog.Common
import Geolog.Core
import Geolog.Diagnostics
import Geolog.Diagnostics.Code qualified as C
import Geolog.Evaluation hiding (bind)
import Geolog.Notation (Ntn)
import Geolog.Notation qualified as N
import Geolog.Pretty hiding (bind)
import Lens.Micro.Platform
import Prettyprinter
import Prelude hiding (lookup)

newtype Ctx = Ctx {ctxElts :: Bwd (QName, Any TyV)}

instance Lookup Ctx QName (BId, Any TyV) where
  lookup (Ctx elts) x = go elts 0
   where
    go BwdNil _ = Nothing
    go (es :> (x', va)) i
      | x == x' = Just (i, va)
      | otherwise = go es (i + 1)

type CtxArg = (?ctx :: Ctx)

data DiagCtx = DiagCtx
  { diagCtxReporter :: Reporter
  , diagCtxFile :: File
  }

makeFields ''DiagCtx

type DiagCtxArg = (?diagCtx :: DiagCtx)

type Elab a = (DiagCtxArg, CtxArg, CtxLenArg, EnvArg) => a

data Syn (l :: Level) = Syn (ElG l) (TyV l)

data ElabException = GiveUp
  deriving (Show)

instance Exception ElabException

binding :: (DiagCtxArg) => Ntn -> IO (QName, Ntn)
binding (N.Infix (N.Ident x _) (N.Keyword ":" _) n) = pure (x, n)
binding n = report (N.span n) (C.Expected C.Binding)

annot :: (DiagCtxArg) => Ntn -> IO (Ntn, Ntn)
annot (N.Infix n1 (N.Keyword ":" _) n2) = pure (n1, n2)
annot n = report (N.span n) (C.Expected C.Annot)

bind :: Elab (Sing l -> QName -> TyV l -> (Elab a) -> a)
bind s x va f = let vx = VNeu (FId ?ctxLen) SId in let_ s x vx va f

bindVal :: Elab (Sing l -> QName -> TyV l -> (Elab (ElV l -> a)) -> a)
bindVal s x va f = let vx = VNeu (FId ?ctxLen) SId in let_ s x vx va (f vx)

let_ :: Elab (Sing l -> QName -> ElV l -> TyV l -> (Elab a) -> a)
let_ s x vx va f =
  let ?env = ?env :> (Any s vx)
      ?ctx = Ctx $ ctxElts ?ctx :> (x, Any s va)
      ?ctxLen = ?ctxLen + 1
   in f

report :: (DiagCtxArg) => Span -> C.Code -> IO a
report s c = do
  let n = Note (Just (SourceLoc (?diagCtx ^. file) s)) Nothing
  let d = Diagnostic c [n]
  reportIO (?diagCtx ^. reporter) d
  evaluate $ throw GiveUp

{- | How do we avoid getting trapped in an infinite loop with Code/El?

One option is to pass around another implicit variable about whether or not
we've tried a type yet. This seems hacky.

The thing is, some of the notations for type should really synthesize, morally
speaking.

We could add a new universe at the top which was unmentionable, so that `typ`
was really checking at this type...

Solution: we don't ever need to explicitly elaborate any meta-level types. They
show up as the types of top-level declarations, but never actually get parsed.
So therefore `typ` can just immediately call `chk` at a universe.
-}
theorySyn :: TyG Theory -> Any Syn
theorySyn ga = Any SMeta $ Syn (gTheoryCode ga) VTheoryU

gVar :: (EnvArg) => Sing l -> BId -> ElG l
gVar s i = G (Var i) (extractAt s $ elemAt ?env i)

members :: Elab (Sing l -> [Ntn] -> IO [(QName, TyS l)])
members _ [] = pure []
members s (n : ns) = do
  (x, n') <- binding n
  G sa va <- typ s n'
  ((x, sa) :) <$> bind s x va (members s ns)

setting :: (DiagCtxArg) => QName -> Ntn -> IO Ntn
setting x (N.Infix (N.Field x' sp) (N.Keyword "=" _) n')
  | x == x' = pure n'
  | otherwise = report sp (C.ExpectedField x x')
setting _ n = report (N.span n) (C.UnexpectedNotation "record entry")

elts ::
  forall (l :: Level).
  Elab
    ( Sing l ->
      Env ->
      [(QName, TyS l)] ->
      [Ntn] ->
      IO ([(QName, ElS l)], [(QName, ElV l)])
    )
elts _ _ [] [] = pure ([], [])
elts s e ((x, a) : ms) (n : ns) = do
  n' <- setting x n
  let va = withSingI s $ evalIn e a
  G st vt <- chk s va n'
  (sfs, vfs) <- let_ s x vt va $ elts s (e :> Any s vt) ms ns
  pure ((x, st) : sfs, (x, vt) : vfs)
elts _ _ _ _ = impossible

withNames :: Elab (((NamesArg) => a) -> a)
withNames f = let ?names = fmap fst (ctxElts ?ctx) in f

pp :: (Prt a) => Elab (a -> Doc ann)
pp x = withNames $ prtPrec precTop x

ident :: (DiagCtxArg) => Ntn -> IO QName
ident (N.Ident x _) = pure x
ident n = report (N.span n) (C.UnexpectedNotation "ident")

typ :: Elab (Sing l -> Ntn -> IO (TyG l))
typ s n = case n of
  N.Tuple ns _ -> do
    fs <- Fields <$> members s ns
    pure $ G (Record fs) (VRecord ?env fs)
  _ -> do
    Any _ (Syn g a) <- syn n
    case (s, a) of
      (SQuery, VQueryU) -> pure $ gQueryEl g
      (STheory, VQueryU) -> pure $ gLiftTy QueryInTheory $ gQueryEl g
      (SMeta, VQueryU) -> pure $ gLiftTy QueryInMeta $ gQueryEl g
      (_, VQueryU) ->
        report (N.span n) $ C.OutOfUniverse Query (fromSing s)
      (STheory, VTheoryU) -> pure $ gTheoryEl g
      (SMeta, VTheoryU) -> pure $ gLiftTy TheoryInMeta $ gTheoryEl g
      (_, VTheoryU) ->
        report (N.span n) $ C.OutOfUniverse Theory (fromSing s)
      _ -> report (N.span n) C.SynthesizedNonUniverse

syn :: Elab (Ntn -> IO (Any Syn))
syn n = case n of
  N.Ident x sp -> case lookup ?ctx x of
    Just (i, Any s va) -> pure $ Any s $ Syn (gVar s i) va
    Nothing -> report sp (C.NotInScope x)
  N.App n1 n2 -> do
    Any s (Syn gf vab) <- syn n1
    case s of
      STheory -> case vab of
        VTheoryPi va b -> do
          gt <- chk SQuery va n2
          pure $ Any s $ Syn (gTheoryApp gf gt) (theoryCloApp b (val gt))
        _ -> report (N.span n1) C.CannotApplyNonPi
      SMeta -> case vab of
        VMetaPi va b -> do
          gt <- chk SMeta va n2
          pure $ Any s $ Syn (gMetaApp gf gt) (metaCloApp b (val gt))
        _ -> report (N.span n1) C.CannotApplyNonPi
      _ -> report (N.span n1) C.CannotApplyNonPi
  N.Infix _ (N.Keyword "=>" _) _ -> report (N.span n) (C.MustChk "lambda syntax")
  N.Keyword "Query" _ -> pure $ theorySyn $ G QueryU VQueryU
  N.Infix n1 (N.Keyword "->" _) n2 -> do
    (x, na) <- binding n1
    G sa va <- typ SQuery na
    G sb _ <- bind SQuery x va $ typ STheory n2
    pure $ theorySyn (G (TheoryPi sa (Abs x sb)) (VTheoryPi va (Clo ?env x sb)))
  N.Tuple _ _ -> report (N.span n) (C.MustChk "tuple syntax")
  _ -> unimplemented

chk :: Elab (Sing l -> TyV l -> Ntn -> IO (ElG l))
chk s va n = case va of
  VLiftTy va' li -> do
    g <- chk (liDom li) va' n
    pure $ gLiftEl g li
  VQueryU -> do
    G sb vb <- typ SQuery n
    pure $ G (QueryCode sb) (VQueryCode vb)
  VTheoryU -> do
    G sb vb <- typ STheory n
    pure $ G (TheoryCode sb) (VTheoryCode vb)
  _ -> case n of
    N.Tuple ns _ -> case va of
      VRecord env (Fields ms) -> do
        unless (length ms == length ns) $ do
          report (N.span n) (C.WrongNumberOfFields (length ms) (length ns))
        (sfs, vfs) <- elts s env ms ns
        pure $ G (Cons (Fields sfs)) (VCons (Fields vfs))
      _ -> report (N.span n) (C.TupleFoundAtUnexpectedType $ pp $ quoteAt s va)
    N.Infix n1 (N.Keyword "=>" _) n2 -> case va of
      VTheoryPi vdom (Clo env _ cod) -> do
        x <- ident n1
        body <- bindVal SQuery x vdom $ \vx -> do
          let vcod = withSingI s $ evalIn (env :> Any SQuery vx) cod
          G body _ <- chk s vcod n2
          pure body
        pure $ G (TheoryLam (Abs x body)) (VTheoryLam (Clo ?env x body))
      _ -> report (N.span n) (C.UnexpectedNotation "non-pi type")
    _ -> do
      Any s' (Syn g va') <- syn n
      let sp = N.span n
      case (s', s) of
        (SQuery, SQuery) ->
          tryConv sp s va va' g
        (SQuery, STheory) ->
          tryConv sp s va (VLiftTy va' QueryInTheory) (gLiftEl g QueryInTheory)
        (SQuery, SMeta) ->
          tryConv sp s va (VLiftTy va' QueryInMeta) (gLiftEl g QueryInMeta)
        (STheory, STheory) ->
          tryConv sp s va va' g
        (STheory, SMeta) ->
          tryConv sp s va (VLiftTy va' TheoryInMeta) (gLiftEl g TheoryInMeta)
        (SMeta, SMeta) ->
          tryConv sp s va va' g
        (SPrim, SPrim) ->
          tryConv sp s va va' g
        (SPrim, SMeta) ->
          tryConv sp s va (VLiftTy va' PrimInMeta) (gLiftEl g PrimInMeta)
        _ -> unimplemented

tryConv :: Elab (Span -> Sing l -> TyV l -> TyV l -> ElG l -> IO (ElG l))
tryConv sp s a a' v =
  let ?names = fmap fst (ctxElts ?ctx)
   in case isConv s a a' of
        Success () -> pure v
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

elabTheory :: Elab (Ntn -> IO (QName, Syn Meta))
elabTheory n = do
  (headN, bodyN) <- definition n
  (x, argsN) <- unpackArgs headN
  (args, body) <- go argsN bodyN
  let ty = wrapPis args
  let el = wrapLams args body
  pure $ (x, Syn (G el (eval el)) (eval ty))
 where
  wrapPis :: [(QName, TyS Theory)] -> TyS Meta
  wrapPis [] = TheoryU
  wrapPis ((x, a) : rest) = MetaPi (LiftTy a TheoryInMeta) (Abs x (wrapPis rest))
  wrapLams :: [(QName, TyS Theory)] -> TyS Theory -> ElS Meta
  wrapLams [] body = TheoryCode body
  wrapLams ((x, _) : rest) body = MetaLam (Abs x (wrapLams rest body))
  go :: Elab ([(QName, Ntn)] -> Ntn -> IO ([(QName, TyS Theory)], TyS Theory))
  go [] bodyN = do
    G body _ <- typ STheory bodyN
    pure ([], body)
  go ((x, tyN) : argsN) bodyN = do
    G a va <- typ STheory tyN
    (args, body) <- bind STheory x va $ go argsN bodyN
    pure ((x, a) : args, body)

elabDef :: Elab (Ntn -> IO (QName, Syn Meta))
elabDef n = do
  (headN, bodyN) <- definition n
  (pat, tyN) <- annot headN
  (x, argsN) <- unpackArgs pat
  (args, retTy, body) <- go argsN tyN bodyN
  let ty = wrapPis args retTy
  let el = wrapLams args body
  pure $ (x, Syn (G el (eval el)) (eval ty))
 where
  go ::
    Elab
      ( [(QName, Ntn)] ->
        Ntn ->
        Ntn ->
        IO ([(QName, TyS Theory)], TyS Theory, ElS Theory)
      )
  go [] tyN bodyN = do
    G a va <- typ STheory tyN
    G t _ <- chk STheory va bodyN
    pure ([], a, t)
  go ((x, argTyN) : argsN) tyN bodyN = do
    G a va <- typ STheory argTyN
    (args, ty, body) <- bind STheory x va $ go argsN tyN bodyN
    pure ((x, a) : args, ty, body)
  wrapPis :: [(QName, TyS Theory)] -> TyS Theory -> TyS Meta
  wrapPis [] ty = LiftTy ty TheoryInMeta
  wrapPis ((x, a) : args) ty =
    MetaPi (LiftTy a TheoryInMeta) (Abs x (wrapPis args ty))
  wrapLams :: [(QName, TyS Theory)] -> ElS Theory -> ElS Meta
  wrapLams [] body = LiftEl body TheoryInMeta
  wrapLams ((x, _) : args) body = MetaLam (Abs x (wrapLams args body))

elabDecl :: Elab (Ntn -> IO (QName, Syn Meta))
elabDecl (N.Decl "theory" n _) = elabTheory n
elabDecl (N.Decl "def" n _) = elabDef n
elabDecl n = report (N.span n) (C.Expected C.Declaration)

elabTop :: Reporter -> File -> [Ntn] -> IO [(QName, ElS Meta, TyS Meta)]
elabTop r f =
  let ?env = BwdNil
      ?diagCtx = DiagCtx r f
      ?ctx = Ctx BwdNil
      ?ctxLen = 0
   in go BwdNil
 where
  go ds [] = pure $ toList ds
  go ds (n : ns) = do
    try (elabDecl n) >>= \case
      Right (x, Syn (G t v) va) -> do
        let a = quote va
        let_ SMeta x v va $ go (ds :> (x, t, a)) ns
      Left (_ :: ElabException) -> go ds ns
