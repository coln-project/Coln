module Geolog.Evaluation where

import Control.Monad (forM_, unless)
import Data.Kind (Type)
import Geolog.Common
import Geolog.Core
import Geolog.Pretty
import Prettyprinter

-- Core Operations
--------------------------------------------------------------------------------

class Core (el :: Energy -> Type) (ty :: Energy -> Type) | el -> ty, ty -> el where
  app :: el e -> el K -> el e
  proj :: el e -> QName -> el e
  code :: ty e -> el e
  decode :: Universe -> el e -> ty e
  universe :: Universe -> ty e

instance Core ElS TyS where
  app = App
  proj = Proj
  code = Code
  decode = Decode
  universe = U

appClo :: Clo (b e) -> ElV K -> b e
appClo (Clo _ f) v = f v
appClo (CloConst v) _ = v

appTy :: TyV Kinetic -> ElV Kinetic -> TyV Kinetic
appTy a v = case behavesAs a of
  Just (VPi _ _ cod) -> appClo cod v
  _ -> panic "appTy should only be called on types that behave like pi types"

projTy :: TyV Kinetic -> QName -> ElV Kinetic -> TyV Kinetic
projTy a x v = case behavesAs a of
  Just (VRecord _ names tys) -> go names tys
    where
      go (x' : xs) (TVCons a' tys')
        | x == x' = a'
        | otherwise = go xs (tys' (proj v x'))
      go _ _ =
        panic "projTy should only be called on fields that exist within the record"
  _ -> panic "projTy should only be called on types that behave like records"

instance Core ElV TyV where
  app (VLam _ clo) v = appClo clo v
  app (VNeu n) v = case n.canon of
    ExpandsTo (VLam _ clo) -> appClo clo v
    _ -> panic "a neutral of pi type should have its eta-expansion in its canon field"
  app _ _ = panic "a value of pi type should be a neutral or lambda"

  proj (VCons fs) x = elemAt fs x
  proj (VNeu n) x = case n.canon of
    ExpandsTo (VCons fs) -> elemAt fs x
    _ -> panic "a neutral of record type should have its eta-expansion in its canon field"
  proj _ _ = panic "a value of record type should be a neutral or cons"

  code (VDecode _ n) = VNeu n
  code a = VCode a

  decode :: Universe -> ElV e -> TyV e
  decode _ (VCode a) = a
  decode u (VNeu n) = VDecode u n
  decode _ _ = panic "a value of universe type should be a neutral or an encoding of a type"

  universe = VU

behavesAs :: TyV K -> Maybe (TyV P)
behavesAs (VU u) = Just (VU u)
behavesAs (VDecode u n) = case n.canon of
  BehavesAs v -> Just (decode u v)
  ExpandsTo _ -> Nothing
  TrueNeutral -> Nothing
behavesAs (VPi pv a b) = Just (VPi pv a b)

expandRecord :: Head -> Spine -> [QName] -> TeleV (TyV K) -> Fields (ElV K)
expandRecord h sp xs te = Fields xs (go xs te)
  where
    go [] TVNil = []
    go (x : xs') (TVCons a f) =
      let v = neu a h (SProj sp x)
       in v : go xs' (f v)
    go _ _ = panic "xs and te should have the same length"

neu :: TyV K -> Head -> Spine -> ElV K
neu a h sp =
  let v = VNeu $ Neutral h sp c a
      c = case behavesAs a of
        Just (VU _) -> TrueNeutral
        Just (VRecord _ xs as) -> ExpandsTo $ VCons $ expandRecord h sp xs as
        Just (VPi _ dom _) -> ExpandsTo $ VLam dom $ Clo "x" $ \v' -> app v v'
        Nothing -> TrueNeutral
   in v

local :: TyV K -> FId -> ElV K
local a i = neu a (Local i) SId

-- Evaluation
--------------------------------------------------------------------------------

type GlobalEnvArg = (?globalEnv :: GlobalEnv)

type EnvArg = (?env :: Env)

class Eval (a :: Energy -> Type) (b :: Energy -> Type) | a -> b where
  eval :: (EnvArg, GlobalEnvArg) => a e -> b e

evalIn :: (GlobalEnvArg, Eval a b) => Env -> a e -> b e
evalIn env t = let ?env = env in eval t

evalAbs :: (GlobalEnvArg, EnvArg, Eval a b) => Abs (a e) -> Clo (b e)
evalAbs (Abs x t) = Clo x (\v -> evalIn (?env :> v) t)
evalAbs (AbsConst t) = CloConst (eval t)

instance Eval ElS ElV where
  eval = \case
    Var i -> elemAt ?env i
    GlobalVar c -> case elemAt ?globalEnv c of
      KEntry _ v _ -> v
      PEntry _ v a -> VNeu (Neutral (Global c) SId (BehavesAs v) a)
    Code t -> code $ eval t
    App t1 t2 -> eval t1 `app` eval t2
    Lam dom c -> VLam (eval dom) (evalAbs c)
    Proj t x -> eval t `proj` x
    Cons fs -> VCons $ eval <$> fs

evalTele :: (GlobalEnvArg, EnvArg) => [TyS e] -> TeleV (TyV e)
evalTele [] = TVNil
evalTele (a : as) = TVCons (eval a) (\v -> let ?env = ?env :> v in evalTele as)

instance Eval TyS TyV where
  eval = \case
    U u -> VU u
    Decode u t -> decode u (eval t)
    Pi pv dom cod -> VPi pv (eval dom) (evalAbs cod)
    Record l xs te -> VRecord l xs (evalTele te)

-- Quoting
--------------------------------------------------------------------------------

type CtxLenArg = (?ctxLen :: Int)

class Quote (a :: Energy -> Type) (b :: Energy -> Type) | a -> b where
  quote :: (CtxLenArg) => a e -> b e

quoteId :: (CtxLenArg) => FId -> BId
quoteId (FId i) = BId (?ctxLen - i - 1)

quoteHead :: (CtxLenArg) => Head -> ElS K
quoteHead = \case
  Local i -> Var (quoteId i)
  Global c -> GlobalVar c

quoteSp :: (CtxLenArg) => Spine -> ElS K -> ElS K
quoteSp sp t = case sp of
  SId -> t
  SApp sp' t' -> App (quoteSp sp' t) (quote t')
  SProj sp' x -> Proj (quoteSp sp' t) x

withFresh :: (CtxLenArg) => TyV K -> ((CtxLenArg) => ElV K -> a) -> a
withFresh a f =
  let i = FId ?ctxLen
   in let ?ctxLen = ?ctxLen + 1
       in f (local a i)

quoteClo :: (CtxLenArg, Quote a b) => TyV K -> Clo (a e) -> Abs (b e)
quoteClo a (Clo x f) = Abs x (withFresh a $ \v -> quote (f v))
quoteClo _ (CloConst t) = AbsConst (quote t)

quoteTele :: (CtxLenArg) => TeleV (TyV K) -> [TyS K]
quoteTele TVNil = []
quoteTele (TVCons a f) = quote a : withFresh a (\v -> quoteTele (f v))

instance Quote ElV ElS where
  quote = \case
    VNeu (Neutral h sp _ _) -> quoteSp sp (quoteHead h)
    VCode a -> Code (quote a)
    VLam dom c -> Lam (quote dom) (quoteClo dom c)
    VCons fs -> Cons (quote <$> fs)

instance Quote TyV TyS where
  quote = \case
    VU u -> U u
    VDecode u n -> Decode u (quote (VNeu n))
    VPi pv a b -> Pi pv (quote a) (quoteClo a b)
    VRecord l xs te -> Record l xs (quoteTele te)

-- Definitional equality
--------------------------------------------------------------------------------

-- When we do a definitional equality check, how should we report failure?
-- We should report the two things that we were originally looking at (which is
-- not in this code) and the two things which were provably not equal.

-- We only have access to the right bound names at the point in time that we
-- fail to check these things, so we have to pretty print them there.

data DefEqCheckError
  = UnequalTys ADoc ADoc (Maybe ADoc)
  | --           ty   ty   explanation
    UnequalEls ADoc ADoc (Maybe ADoc)
  | --           el   el   explanation
    UnequalSpines Spine Spine (Maybe ADoc)

-- this error should always be caught

type DefEqM a = Either DefEqCheckError ()

throwUnequalTys ::
  (NamesArg, CtxLenArg) =>
  TyV K -> TyV K -> Maybe ADoc -> DefEqM ()
throwUnequalTys a a' e =
  Left (UnequalTys (prtTop $ quote a) (prtTop $ quote a') e)

throwUnequalEls ::
  (NamesArg, CtxLenArg) =>
  ElV K -> ElV K -> Maybe ADoc -> DefEqM ()
throwUnequalEls v v' e =
  Left (UnequalEls (prtTop $ quote v) (prtTop $ quote v') e)

throwUnequalSpines :: Spine -> Spine -> Maybe ADoc -> DefEqM ()
throwUnequalSpines sp sp' e = Left $ UnequalSpines sp sp' e

withFresh' ::
  (NamesArg, CtxLenArg) =>
  TyV K -> ((NamesArg, CtxLenArg) => ElV K -> a) -> a
withFresh' a f =
  let i = FId ?ctxLen
   in let ?ctxLen = ?ctxLen + 1
          ?names = ?names :> "x"
       in f $ local a i

class DefEq a where
  defEq :: (NamesArg, CtxLenArg) => a -> a -> DefEqM ()

instance DefEq (TyV K) where
  defEq a a' = case (a, a') of
    (VU u, VU u') | u == u' -> pure ()
    (VDecode _ n, VDecode _ n') -> defEq n n'
    (VPi pv dom cod, VPi pv' dom' cod') -> do
      unless (pv == pv') $
        throwUnequalTys a a' $
          Just $
            "different pi variants:" <+> pretty pv <+> "and" <+> pretty pv'
      defEq dom dom'
      withFresh' dom $ \v -> defEq (appClo cod v) (appClo cod' v)
    _ -> throwUnequalTys a a' Nothing

prtHead :: (NamesArg, CtxLenArg) => Head -> ADoc
prtHead (Local i) = prtTop $ quoteId i
prtHead (Global (Constant x)) = pretty x

instance DefEq Neutral where
  defEq n n' = do
    unless (n.head == n'.head) $
      throwUnequalEls (VNeu n) (VNeu n') $
        Just $
          "different heads for neutral:"
            <+> prtHead n.head
            <+> "and"
            <+> prtHead n'.head
    defEq n.spine n'.spine

instance DefEq Spine where
  defEq sp sp' = case (sp, sp') of
    (SId, SId) -> pure ()
    (SApp sq v, SApp sq' v') -> do
      defEq sq sq'
      defEq v v'
    (SProj sq x, SProj sq' x') -> do
      defEq sq sq'
      unless (x == x') $
        throwUnequalSpines sq sq' $
          Just $
            "fields are not equal:" <+> pretty x <+> "and" <+> pretty x'
    _ -> throwUnequalSpines sp sp' Nothing

canon :: ElV K -> ElV K
canon v@(VNeu n)
  | ExpandsTo v' <- n.canon = v'
  | otherwise = v
canon v = v

instance DefEq (ElV K) where
  -- Note: we expect v and v' to be eta-expanded here, because we reflect
  defEq v v' = case (canon v, canon v') of
    (VNeu n, VNeu n') -> defEq n n'
    (VCode a, VCode a') -> defEq a a'
    (VLam a c, VLam _ c') ->
      withFresh a $ \w -> defEq (appClo c v) (appClo c' w)
    (VCons (Fields _ vs), VCons (Fields _ vs')) ->
      forM_ (zip vs vs') (uncurry defEq)
    _ -> throwUnequalEls v v' Nothing
