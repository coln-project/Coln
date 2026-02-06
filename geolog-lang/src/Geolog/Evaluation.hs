module Geolog.Evaluation where

import Control.Monad (unless)
import Geolog.Common
import Geolog.Core
import Geolog.Pretty
import Prettyprinter

-- Core Operations
--------------------------------------------------------------------------------

class Core el ty | el -> ty, ty -> el where
  app :: el -> el -> el
  proj :: el -> QName -> el
  code :: ty -> el
  decode :: Universe -> el -> ty
  universe :: Universe -> ty

instance Core ElS TyS where
  app = App
  proj = Proj
  code = Code
  decode = Decode
  universe = U

appClo :: (Eval a b) => Clo a -> ElV -> b
appClo (Clo env _ t) v = evalIn (env :> v) t

instance Core ElV TyV where
  app (VLam clo) v = appClo clo v
  app (VNeu i sp) v = VNeu i (SApp sp v)
  app _ _ = impossible

  proj (VCons fs) x = elemAt fs x
  proj (VNeu i sp) x = VNeu i (SProj sp x)
  proj _ _ = impossible
  
  code (VDecode _ v) = v
  code va = VCode va
  
  decode _ (VCode va) = va
  decode u v = VDecode u v

  universe = VU

-- Evaluation
--------------------------------------------------------------------------------

type EnvArg = (?env :: Env)

class Eval a b | a -> b where
  eval :: EnvArg => a -> b

evalIn :: (Eval a b) => Env -> a -> b
evalIn env t = let ?env = env in eval t

instance Eval ElS ElV where
  eval = \case
    Var i -> elemAt ?env i
    Code t -> code $ eval t
    App t1 t2 -> app (eval t1) (eval t2)
    Lam (Abs x t) -> VLam (Clo ?env x t)
    Proj t x -> proj (eval t) x
    Cons fs -> VCons $ eval <$> fs

instance Eval TyS TyV where
  eval = \case
    U u -> VU u
    Decode u t -> decode u (eval t)
    Pi pv a (Abs x b) -> VPi pv (eval a) (Clo ?env x b)
    Record l fs -> VRecord l ?env fs

-- Quoting
--------------------------------------------------------------------------------

type CtxLenArg = (?ctxLen :: Int)

class Quote a b | a -> b where
  quote :: CtxLenArg => a -> b

fresh :: CtxLenArg => FId
fresh = FId ?ctxLen

withFresh :: (CtxLenArg) => ((CtxLenArg) => ElV -> a) -> a
withFresh f =
  let v = VNeu fresh SId in
    let ?ctxLen = ?ctxLen + 1 in
      f v

instance Quote FId BId where
  quote (FId i) = BId (?ctxLen - i - 1)

instance Quote Spine (ElS -> ElS) where
  quote sp t = case sp of
    SId -> t
    SApp sp' v -> App (quote sp' t) (quote v)
    SProj sp' x -> Proj (quote sp' t) x

instance Quote ElV ElS where
  quote = \case
    VNeu i sp -> quote sp $ Var (quote i)
    VCode va -> Code (quote va)
    VLam (Clo env x t) -> withFresh $ \v ->
      Lam $ Abs x $ quote $ evalIn (env :> v) t
    VCons fs -> Cons $ quote <$> fs

instance Quote TyV TyS where
  quote = \case
    VU u -> U u
    VDecode u v -> Decode u (quote v)
    VPi pv a (Clo env x b) -> Pi pv (quote a) $ withFresh $ \v ->
      Abs x $ quote $ evalIn (env :> v) b
    VRecord l env (Fields fs) -> Record l $ Fields $ go fs env
      where
        go :: (CtxLenArg) => [(QName, TyS)] -> Env -> [(QName, TyS)]
        go [] _ = []
        go ((x, a) : rest) e = (x, a') : rest'
          where
            a' = quote $ evalIn e a
            rest' = withFresh $ \v -> go rest (e :> v)

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

withNamedFresh :: (ConvCtx) => QName -> ((ConvCtx) => ElV -> a) -> a
withNamedFresh x f =
  let v = VNeu fresh SId in
    let ?ctxLen = ?ctxLen + 1
        ?names = ?names :> x in f v

convFail :: (ConvCtx) => TyV -> TyV -> Doc Ann -> ConvM a
convFail a b d =
  Failure ( NotConvertableTy (prtTop $ quote a) (prtTop $ quote b)) d

convElFail :: (ConvCtx) => ElV -> ElV -> Doc Ann -> ConvM a
convElFail a b d =
  Failure ( NotConvertableEl (prtTop $ quote a) (prtTop $ quote b)) d

isConvSp :: (ConvCtx) => FId -> Spine -> Spine -> ConvM ()
isConvSp _ SId SId = pure ()
isConvSp i (SApp sp v) (SApp sp' v') = do
  isConvSp i sp sp'
  isConvEl v v'
isConvSp i (SProj sp x) (SProj sp' x') = do
  isConvSp i sp sp'
  unless (x == x') $
    convElFail
      (VNeu i (SProj sp x))
      (VNeu i (SProj sp x))
      "projecting from non-equal fields"
isConvSp i sp sp' =
  convElFail (VNeu i sp) (VNeu i sp') "mismatching spine heads"

isConvElts :: (ConvCtx) => [(QName, ElV, ElV)] -> ConvM ()
isConvElts [] = pure ()
isConvElts ((_, v, v') : es) = do
  isConvEl v v'
  isConvElts es

zipFields :: [(QName, a)] -> [(QName, a)] -> Maybe [(QName, a, a)]
zipFields [] [] = Just []
zipFields ((x, a) : ms) ((x', a') : ms')
  | x == x' = ((x, a, a') :) <$> (zipFields ms ms')
  | otherwise = Nothing
zipFields _ _ = Nothing

-- TODO: type-directed conversion checking with eta expansion
isConvEl :: (ConvCtx) => ElV -> ElV -> ConvM ()
isConvEl v v' = case (v, v') of
  (VNeu i sp, VNeu i' sp') -> do
    unless (i == i') $ convElFail v v' "heads of neutrals do not match"
    isConvSp i sp sp'
  (VCode ty, VCode ty') -> isConv ty ty'
  (VLam clo, VLam clo') -> do
    withNamedFresh "x" $ \vx -> isConvEl (appClo clo vx) (appClo clo' vx)
  (VCons (Fields ms), VCons (Fields ms')) -> case zipFields ms ms' of
    Just combined -> isConvElts combined
    Nothing -> convElFail v v' "differing fields"
  _ -> convElFail v v' "different element heads"

isConvTele :: (ConvCtx) => Env -> Env -> [(QName, TyS, TyS)] -> ConvM ()
isConvTele _ _ [] = pure ()
isConvTele e e' ((x, a, a') : ms) = do
  isConv (evalIn e a) (evalIn e' a')
  withNamedFresh x $ \vx -> isConvTele (e :> vx) (e' :> vx) ms
  
isConv :: (ConvCtx) => TyV -> TyV -> ConvM ()
isConv a a' = case (a, a') of 
  (VU u, VU u') -> unless (u == u') $ convFail a a' "different universes"
  (VDecode _ v, VDecode _ v') -> isConvEl v v'
  (VPi pv dom cod, VPi pv' dom' cod') -> do
    unless (pv == pv') $ convFail a a' "different pi variants"
    isConv dom dom'
    withNamedFresh "x" $ \vx -> isConv (appClo cod vx) (appClo cod' vx)
  (VRecord l e (Fields ms), VRecord l' e' (Fields ms')) -> do
    unless (l == l') $ convFail a a' "record types at different levels"
    case zipFields ms ms' of
      Just combined -> isConvTele e e' combined
      Nothing -> convFail a a' "record types have different fields"
  _ -> convFail a a' "different type heads"
