module Geolog.Evaluation where

import Control.Monad (unless)
import Data.Singletons
import Geolog.Common
import Geolog.Core
import Geolog.Pretty hiding (bind)
import Prettyprinter

-- Implicit arguments
--------------------------------------------------------------------------------

type EnvArg = (?env :: Env)

type CtxLenArg = (?ctxLen :: Int)

-- Type classes
--------------------------------------------------------------------------------

class Eval a b | a -> b where
  eval :: (EnvArg) => (SingI l) => a l -> b l

evalIn :: (Eval a b) => (SingI l) => Env -> a l -> b l
evalIn env t = let ?env = env in eval t

class Quote a b | a -> b where
  quote :: (CtxLenArg) => (SingI l) => a l -> b l

quoteAt :: (Quote a b) => (CtxLenArg) => Sing l -> a l -> b l
quoteAt s x = withSingI s (quote x)

-- Utilities
--------------------------------------------------------------------------------

fresh :: (CtxLenArg) => FId
fresh = FId ?ctxLen

-- TODO: we reuse the word "bind" in Pretty, Evaluation, Elaboration; we should
-- probably call these different things
bind :: (CtxLenArg) => Sing (l :: Level) -> ((CtxLenArg) => Any ElV -> a) -> a
bind s f =
  let v = Any s (VNeu fresh SId)
   in let ?ctxLen = ?ctxLen + 1 in f v

theoryCloApp :: (Eval a b) => Clo a Theory -> ElV Query -> b Theory
theoryCloApp (Clo env _ body) v = evalIn (env :> Any SQuery v) body

metaCloApp :: (Eval a b) => Clo a Meta -> ElV Meta -> b Meta
metaCloApp (Clo env _ body) v = evalIn (env :> Any SMeta v) body

theoryApp :: ElV Theory -> ElV Query -> ElV Theory
theoryApp (VTheoryLam clo) x = theoryCloApp clo x
theoryApp (VNeu i sp) x = VNeu i (STheoryApp sp x)
theoryApp _ _ = impossible

metaApp :: ElV Meta -> ElV Meta -> ElV Meta
metaApp (VMetaLam clo) x = metaCloApp clo x
metaApp (VNeu i sp) x = VNeu i (SMetaApp sp x)
metaApp _ _ = impossible

-- TODO: this seems like a premature optimization, also it messes with the
-- naming scheme where lowercase functions named the same as syntax constructors
-- act on values.
queryCode :: TyS Query -> ElS Theory
queryCode (QueryEl t) = t
queryCode ty = QueryCode ty

queryEl :: ElS Theory -> TyS Query
queryEl (QueryCode ty) = ty
queryEl t = QueryEl t

theoryCode :: TyS Theory -> ElS Meta
theoryCode (TheoryEl t) = t
theoryCode ty = TheoryCode ty

theoryEl :: ElS Meta -> TyS Theory
theoryEl (TheoryCode ty) = ty
theoryEl t = TheoryEl t

vQueryCode :: TyV Query -> ElV Theory
vQueryCode (VQueryEl t) = t
vQueryCode ty = VQueryCode ty

vQueryEl :: ElV Theory -> TyV Query
vQueryEl (VQueryCode ty) = ty
vQueryEl t = VQueryEl t

vTheoryCode :: TyV Theory -> ElV Meta
vTheoryCode (VTheoryEl t) = t
vTheoryCode ty = VTheoryCode ty

vTheoryEl :: ElV Meta -> TyV Theory
vTheoryEl (VTheoryCode ty) = ty
vTheoryEl t = VTheoryEl t

proj :: (SingI l) => ElV l -> QName -> ElV l
proj (VCons fs) x = elemAt fs x
proj (VNeu i sp) x = VNeu i (SProj sp x)
proj _ _ = impossible

-- Glued operations
--------------------------------------------------------------------------------

-- NOTE: this is glued evaluation in a different sense than typically used!
-- Typically, glued evaluation pairs a value with a
-- more-eta-expanded-and-beta-reduced value. This glued evalution pairs syntax
-- with a value. These glued values are the results of elaboration, which
-- ensures that we don't have to re-evaluate syntax more than once. In
-- pathological cases, not doing this can cause elaboration to be asymptotically
-- slower.

data Glued s v (l :: Level) = G {stx :: (s l), val :: ~(v l)}

type ElG = Glued ElS ElV

type TyG = Glued TyS TyV

gLiftTy :: LevelInclusion l l' -> TyG l -> TyG l'
gLiftTy li (G s v) = G (LiftTy s li) (VLiftTy v li)

gQueryCode :: TyG Query -> ElG Theory
gQueryCode (G sa va) = G (queryCode sa) (vQueryCode va)

gQueryEl :: ElG Theory -> TyG Query
gQueryEl (G sa va) = G (queryEl sa) (vQueryEl va)

gTheoryCode :: TyG Theory -> ElG Meta
gTheoryCode (G sa va) = G (theoryCode sa) (vTheoryCode va)

gTheoryEl :: ElG Meta -> TyG Theory
gTheoryEl (G sa va) = G (theoryEl sa) (vTheoryEl va)

gTheoryApp :: ElG Theory -> ElG Query -> ElG Theory
gTheoryApp (G sf vf) (G st vt) = G (TheoryApp sf st) (theoryApp vf vt)

gMetaApp :: ElG Meta -> ElG Meta -> ElG Meta
gMetaApp (G sf vf) (G st vt) = G (MetaApp sf st) (metaApp vf vt)

gLiftEl :: ElG l -> LevelInclusion l l' -> ElG l'
gLiftEl (G s v) li = G (LiftEl s li) (VLiftEl v li)

-- Evaluation
--------------------------------------------------------------------------------

instance Eval (Fields ElS) (Fields ElV) where
  eval (Fields fs) = Fields $ [(x, eval t) | (x, t) <- fs]

instance Eval ElS ElV where
  eval = \case
    Var i -> extract $ elemAt ?env i
    QueryCode ty -> vQueryCode $ eval ty
    TheoryCode ty -> vTheoryCode $ eval ty
    TheoryApp f t -> theoryApp (eval f) (eval t)
    MetaApp f t -> metaApp (eval f) (eval t)
    TheoryLam body -> VTheoryLam $ eval body
    MetaLam body -> VMetaLam $ eval body
    Proj t x -> proj (eval t) x
    Cons fields -> VCons $ eval fields
    LiftEl t li -> VLiftEl (withDom li $ eval t) li

instance Eval (Abs f) (Clo f) where
  eval (Abs x a) = Clo ?env x a

instance Eval TyS TyV where
  eval = \case
    QueryU -> VQueryU
    QueryEl t -> vQueryEl (eval t)
    TheoryU -> VTheoryU
    TheoryEl t -> vTheoryEl (eval t)
    TheoryPi a b -> VTheoryPi (eval a) (eval b)
    MetaPi a b -> VMetaPi (eval a) (eval b)
    Record fields -> VRecord ?env fields
    LiftTy a li -> VLiftTy (withDom li $ eval a) li

-- Quoting
--------------------------------------------------------------------------------

type Const a l = a

quoteSp :: (CtxLenArg) => (SingI l) => Spine l -> ElS l -> ElS l
quoteSp sp t = case sp of
  SId -> t
  STheoryApp sp' v -> TheoryApp (quoteSp sp' t) (quote v)
  SMetaApp sp' v -> MetaApp (quoteSp sp' t) (quote v)
  SProj sp' x -> Proj (quoteSp sp' t) x

quoteId :: (CtxLenArg) => FId -> BId
quoteId (FId i) = BId (?ctxLen - i - 1)

instance Quote ElV ElS where
  quote = \case
    VNeu i sp -> quoteSp sp $ Var $ quoteId i
    VQueryCode a -> QueryCode (quote a)
    VTheoryCode a -> TheoryCode (quote a)
    VLiftEl t li -> LiftEl (withDom li $ quote t) li
    VTheoryLam (Clo env x a) -> bind SQuery $ \v ->
      TheoryLam $ Abs x $ quote $ evalIn (env :> v) a
    VMetaLam (Clo env x a) -> bind SMeta $ \v ->
      MetaLam $ Abs x $ quote $ evalIn (env :> v) a
    VCons (Fields fs) -> Cons $ Fields [(x, quote v) | (x, v) <- fs]

instance Quote TyV TyS where
  quote = \case
    VQueryU -> QueryU
    VQueryEl e -> QueryEl (quote e)
    VTheoryU -> TheoryU
    VTheoryEl e -> TheoryEl (quote e)
    VTheoryPi a (Clo env x b) -> TheoryPi (quote a) $ bind SQuery $ \v ->
      Abs x $ quote $ evalIn (env :> v) b
    VMetaPi a (Clo env x b) -> MetaPi (quote a) $ bind SMeta $ \v ->
      Abs x $ quote $ evalIn (env :> v) b
    VRecord env (Fields fs) -> Record $ Fields $ go fs env
      where
        go :: forall l. (CtxLenArg) => (SingI l) => [(QName, TyS l)] -> Env -> [(QName, TyS l)]
        go [] _ = []
        go ((x, a) : rest) e = (x, a') : rest'
          where
            a' = quote $ evalIn e a
            rest' = bind @l sing $ \v -> go rest (e :> v)
    VLiftTy a li -> LiftTy (withDom li $ quote a) li

demoteEl :: Sing l -> ElV l -> Any ElV
demoteEl _ (VLiftEl v li) = demoteEl (liDom li) v
demoteEl _ (VQueryCode ty) = case demoteTy SQuery ty of
  Any SQuery ty' -> Any STheory (vQueryCode ty')
  _ -> impossible
demoteEl _ (VTheoryCode ty) = case demoteTy STheory ty of
  Any SQuery ty' -> Any STheory (vQueryCode ty')
  Any STheory ty' -> Any SMeta (vTheoryCode ty')
  _ -> impossible
demoteEl s v = Any s v

demoteTy :: Sing l -> TyV l -> Any TyV
demoteTy _ (VLiftTy ty li) = demoteTy (liDom li) ty
demoteTy _ (VQueryEl v) = case demoteEl STheory v of
  Any STheory v' -> Any SQuery (vQueryEl v')
  _ -> impossible
demoteTy _ (VTheoryEl v) = case demoteEl SMeta v of
  Any STheory v' -> Any SQuery (vQueryEl v')
  Any SMeta v' -> Any STheory (vTheoryEl v')
  _ -> impossible
demoteTy s v = Any s v

-- Conversion checking
--------------------------------------------------------------------------------

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

isConvSp :: (ConvCtx) => Sing l -> FId -> Spine l -> Spine l -> ConvM ()
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
