module Geolog.Evaluation where

import Data.Singletons
import Geolog.Common
import Geolog.Core

type EnvArg = (?env :: Env)

type CtxLenArg = (?ctxLen :: Int)

fresh :: (CtxLenArg) => FId
fresh = FId ?ctxLen

bind :: (CtxLenArg) => Sing (l :: Level) -> ((CtxLenArg) => Any ElV -> a) -> a
bind s f =
  let v = Any s (VNeu fresh SId)
   in let ?ctxLen = ?ctxLen + 1 in f v

class Eval a b | a -> b where
  eval :: (EnvArg) => (SingI l) => a l -> b l

evalIn :: (Eval a b) => (SingI l) => Env -> a l -> b l
evalIn env t = let ?env = env in eval t

theoryApp :: ElV Theory -> ElV Query -> ElV Theory
theoryApp = undefined

metaApp :: ElV Meta -> ElV Meta -> ElV Meta
metaApp = undefined

proj :: (SingI l) => ElV l -> QName -> ElV l
proj = undefined

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

class Quote a b | a -> b where
  quote :: (CtxLenArg) => (SingI l) => a l -> b l

type Const a l = a

quoteSp :: (CtxLenArg) => (SingI l) => Sp l -> ElS l -> ElS l
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
