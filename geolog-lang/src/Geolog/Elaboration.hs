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

data Glued s v (l :: Level) = G {stx :: (s l), val :: ~(v l)}

type ElG = Glued ElS ElV

type TyG = Glued TyS TyV

gLiftTy :: LevelInclusion l l' -> TyG l -> TyG l'
gLiftTy li (G s v) = G (LiftTy s li) (VLiftTy v li)

data Syn (l :: Level) = Syn (ElG l) (TyV l)

data ElabException = GiveUp
  deriving (Show)

instance Exception ElabException

annot :: Ntn -> (QName, Ntn)
annot (N.Infix (N.Ident x _) (N.Keyword ":" _) n) = (x, n)
annot n = ("_", n)

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
gQueryCode :: TyG Query -> ElG Theory
gQueryCode (G sa va) = G (queryCode sa) (vQueryCode va)

gQueryEl :: ElG Theory -> TyG Query
gQueryEl (G sa va) = G (queryEl sa) (vQueryEl va)

gTheoryCode :: TyG Theory -> ElG Meta
gTheoryCode (G sa va) = G (theoryCode sa) (vTheoryCode va)

gTheoryEl :: ElG Meta -> TyG Theory
gTheoryEl (G sa va) = G (theoryEl sa) (vTheoryEl va)

theorySyn :: TyG Theory -> Any Syn
theorySyn ga = Any SMeta $ Syn (gTheoryCode ga) VTheoryU

gVar :: (EnvArg) => Sing l -> BId -> ElG l
gVar s i = G (Var i) (extractAt s $ elemAt ?env i)

gTheoryApp :: ElG Theory -> ElG Query -> ElG Theory
gTheoryApp (G sf vf) (G st vt) = G (TheoryApp sf st) (theoryApp vf vt)

gMetaApp :: ElG Meta -> ElG Meta -> ElG Meta
gMetaApp (G sf vf) (G st vt) = G (MetaApp sf st) (metaApp vf vt)

theoryCloApp :: (Eval a b) => Clo a Theory -> ElV Query -> b Theory
theoryCloApp (Clo env _ body) v = evalIn (env :> Any SQuery v) body

metaCloApp :: (Eval a b) => Clo a Meta -> ElV Meta -> b Meta
metaCloApp (Clo env _ body) v = evalIn (env :> Any SMeta v) body

members :: Elab (Sing l -> [Ntn] -> IO [(QName, TyS l)])
members _ [] = pure []
members s (n : ns) = do
  let (x, n') = annot n
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

gLiftEl :: ElG l -> LevelInclusion l l' -> ElG l'
gLiftEl (G s v) li = G (LiftEl s li) (VLiftEl v li)

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
    let (x, na) = annot n1
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

-- We have to quote and pretty-print at the point of conversion failure because
-- that's when we have access to all the names
data ConvFailure
  = NotConvertableEl (Doc Ann) (Doc Ann)
  | NotConvertableTy (Doc Ann) (Doc Ann)

data ConvM a = Success a | Failure ConvFailure (Doc Ann)
  deriving (Functor)

instance Applicative ConvM where
  pure = Success
  mf <*> mx = case mf of
    Success f -> case mx of
      Success x -> Success $ f x
      Failure t e -> Failure t e
    Failure t e -> Failure t e

instance Monad ConvM where
  mx >>= f = case mx of
    Success x -> f x
    Failure t e -> Failure t e

type ConvCtx = (NamesArg, CtxLenArg)

convFail :: (ConvCtx) => Any TyV -> Any TyV -> Doc Ann -> ConvM a
convFail (Any sa a) (Any sb b) d =
  Failure
    ( NotConvertableTy
        (prtTop $ withSingI sa $ quote a)
        (prtTop $ withSingI sb $ quote b)
    )
    d

convElFail :: (ConvCtx) => Any ElV -> Any ElV -> Doc Ann -> ConvM a
convElFail (Any sa a) (Any sb b) d =
  Failure
    ( NotConvertableEl
        (prtTop $ withSingI sa $ quote a)
        (prtTop $ withSingI sb $ quote b)
    )
    d

isConvSp :: (ConvCtx) => Sing l -> FId -> Sp l -> Sp l -> ConvM ()
isConvSp _ _ SId SId = pure ()
isConvSp s i (STheoryApp sp v) (STheoryApp sp' v') = do
  isConvSp s i sp sp'
  isConvEl SQuery v v'
isConvSp s i (SMetaApp sp v) (SMetaApp sp' v') = do
  isConvSp s i sp sp'
  isConvEl SMeta v v'
isConvSp s i (SProj sp x) (SProj sp' x') = do
  isConvSp s i sp sp'
  unless (x == x') $
    convElFail
      (Any s (VNeu i (SProj sp x)))
      (Any s (VNeu i (SProj sp x)))
      "projecting from non-equal fields"
isConvSp s i sp sp' =
  convElFail (Any s (VNeu i sp)) (Any s (VNeu i sp')) "mismatching spine heads"

isConvElts :: (ConvCtx) => Sing l -> [(QName, ElV l, ElV l)] -> ConvM ()
isConvElts _ [] = pure ()
isConvElts s ((_, v, v') : es) = do
  isConvEl s v v'
  isConvElts s es

zipFields :: [(QName, a)] -> [(QName, a)] -> Maybe [(QName, a, a)]
zipFields [] [] = Just []
zipFields ((x, a) : ms) ((x', a') : ms')
  | x == x' = ((x, a, a') :) <$> (zipFields ms ms')
  | otherwise = Nothing
zipFields _ _ = Nothing

-- TODO: type-directed conversion checking with eta expansion
isConvEl :: (ConvCtx) => Sing l -> ElV l -> ElV l -> ConvM ()
isConvEl s v v' = case (v, v') of
  (VNeu i sp, VNeu i' sp') -> do
    unless (i == i') $ convElFail (Any s v) (Any s v') "heads of neutrals do not match"
    isConvSp s i sp sp'
  (VQueryCode ty, VQueryCode ty') -> isConv SQuery ty ty'
  (VTheoryCode ty, VTheoryCode ty') -> isConv STheory ty ty'
  (VLiftEl w li, VLiftEl w' li') -> case (li, li') of
    (QueryInTheory, QueryInTheory) -> isConvEl SQuery w w'
    (QueryInMeta, QueryInMeta) -> isConvEl SQuery w w'
    (TheoryInMeta, TheoryInMeta) -> isConvEl STheory w w'
    (PrimInMeta, PrimInMeta) -> isConvEl SPrim w w'
    _ -> convElFail (Any s v) (Any s v) "lifts from different levels"
  (VTheoryLam clo, VTheoryLam clo') -> do
    withFresh "x" $ \vx -> isConvEl STheory (theoryCloApp clo vx) (theoryCloApp clo' vx)
  (VMetaLam clo, VMetaLam clo') -> do
    withFresh "x" $ \vx -> isConvEl SMeta (metaCloApp clo vx) (metaCloApp clo' vx)
  (VCons (Fields ms), VCons (Fields ms')) -> case zipFields ms ms' of
    Just combined -> isConvElts s combined
    Nothing -> convElFail (Any s v) (Any s v') "differing fields"
  _ -> convElFail (Any s v) (Any s v') ""

withFresh :: (ConvCtx) => QName -> ((ConvCtx) => ElV l -> a) -> a
withFresh x f =
  let vx = VNeu (FId ?ctxLen) SId
   in let ?ctxLen = ?ctxLen + 1
          ?names = ?names :> x
       in f vx

isConv :: (ConvCtx) => Sing l -> TyV l -> TyV l -> ConvM ()
isConv s a b = case (demoteTy s a, demoteTy s b) of
  (Any SQuery a', Any SQuery b') -> isConv' SQuery a' b'
  (Any STheory a', Any STheory b') -> isConv' STheory a' b'
  (Any SMeta a', Any SMeta b') -> isConv' SMeta a' b'
  (Any SPrim a', Any SPrim b') -> isConv' SPrim a' b'
  (a', b') ->
    convFail a' b' $
      "demoted types are at different levels:"
        <+> pretty (levelOf a')
        <+> "and"
        <+> pretty (levelOf b')

isConvTele :: (ConvCtx) => Sing l -> Env -> Env -> [(QName, TyS l, TyS l)] -> ConvM ()
isConvTele _ _ _ [] = pure ()
isConvTele s e e' ((x, a, a') : ms) = do
  let va = withSingI s $ evalIn e a
  let va' = withSingI s $ evalIn e' a'
  isConv s va va'
  withFresh x $ \vx -> isConvTele s (e :> Any s vx) (e' :> Any s vx) ms

-- Assumes that both types are already demoted
isConv' :: (ConvCtx) => Sing l -> TyV l -> TyV l -> ConvM ()
isConv' s a a' = case (a, a') of
  (VQueryU, VQueryU) -> pure ()
  (VQueryEl v, VQueryEl v') -> isConvEl STheory v v'
  (VTheoryU, VTheoryU) -> pure ()
  (VTheoryEl v, VTheoryEl v') -> isConvEl SMeta v v'
  (VTheoryPi dom cod, VTheoryPi dom' cod') -> do
    isConv SQuery dom dom'
    withFresh "x" $ \vx -> isConv STheory (theoryCloApp cod vx) (theoryCloApp cod' vx)
  (VMetaPi dom cod, VMetaPi dom' cod') -> do
    isConv SMeta dom dom'
    withFresh "x" $ \vx -> isConv SMeta (metaCloApp cod vx) (metaCloApp cod' vx)
  (VRecord e (Fields ms), VRecord e' (Fields ms')) -> case zipFields ms ms' of
    Just combined -> isConvTele s e e' combined
    Nothing -> convFail (Any s a) (Any s a') "record types have different fields"
  (VLiftTy _ _, _) -> impossible
  (_, VLiftTy _ _) -> impossible
  _ -> unimplemented
