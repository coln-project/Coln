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

thApp :: ElV Th -> ElV Sort -> ElV Th
thApp = undefined

setApp :: ElV Set -> ElV Set -> ElV Set
setApp = undefined

proj :: (SingI l) => ElV l -> QName -> ElV l
proj = undefined

instance Eval (Fields ElS) (Fields ElV) where
  eval (Fields fs) = Fields $ [(x, eval t) | (x, t) <- fs]

instance Eval ElS ElV where
  eval = \case
    Var i -> extract $ elemAt ?env i
    SortCode ty -> VSortCode $ eval ty
    ThCode ty -> VThCode $ eval ty
    ThApp f t -> thApp (eval f) (eval t)
    SetApp f t -> setApp (eval f) (eval t)
    ThLam body -> VThLam $ eval body
    SetLam body -> VSetLam $ eval body
    Proj t x -> proj (eval t) x
    Cons fields -> VCons $ eval fields
    LiftEl t li -> VLiftEl (withDom li $ eval t) li

sortEl :: ElV Th -> TyV Sort
sortEl (VSortCode ty) = ty
sortEl v = VSortEl v

thEl :: ElV Set -> TyV Th
thEl (VThCode ty) = ty
thEl v = VThEl v

instance Eval (Abs f) (Clo f) where
  eval (Abs x a) = Clo ?env x a

instance Eval TyS TyV where
  eval = \case
    SortU -> VSortU
    SortEl t -> sortEl (eval t)
    ThU -> VThU
    ThEl t -> thEl (eval t)
    ThPi a b -> VThPi (eval a) (eval b)
    SetPi a b -> VSetPi (eval a) (eval b)
    Record fields -> VRecord ?env fields
    LiftTy a li -> VLiftTy (withDom li $ eval a) li

class Quote a b | a -> b where
  quote :: (CtxLenArg) => (SingI l) => a l -> b l

type Const a l = a

quoteSp :: (CtxLenArg) => (SingI l) => Sp l -> ElS l -> ElS l
quoteSp sp t = case sp of
  SId -> t
  SThApp sp' v -> ThApp (quoteSp sp' t) (quote v)
  SSetApp sp' v -> SetApp (quoteSp sp' t) (quote v)
  SProj sp' x -> Proj (quoteSp sp' t) x

quoteId :: (CtxLenArg) => FId -> BId
quoteId (FId i) = BId (?ctxLen - i - 1)

instance Quote ElV ElS where
  quote = \case
    VNeu i sp -> quoteSp sp $ Var $ quoteId i
    VSortCode a -> SortCode (quote a)
    VThCode a -> ThCode (quote a)
    VLiftEl t li -> LiftEl (withDom li $ quote t) li
    VThLam (Clo env x a) -> bind SSort $ \v ->
      ThLam $ Abs x $ quote $ evalIn (env :> v) a
    VSetLam (Clo env x a) -> bind SSet $ \v ->
      SetLam $ Abs x $ quote $ evalIn (env :> v) a
    VCons (Fields fs) -> Cons $ Fields [(x, quote v) | (x, v) <- fs]

instance Quote TyV TyS where
  quote = \case
    VSortU -> SortU
    VSortEl e -> SortEl (quote e)
    VThU -> ThU
    VThEl e -> ThEl (quote e)
    VThPi a (Clo env x b) -> ThPi (quote a) $ bind SSort $ \v ->
      Abs x $ quote $ evalIn (env :> v) b
    VSetPi a (Clo env x b) -> SetPi (quote a) $ bind SSet $ \v ->
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
