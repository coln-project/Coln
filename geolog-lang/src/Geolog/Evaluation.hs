module Geolog.Evaluation where

import Data.Kind (Type)
import Geolog.Common
import Geolog.Core

-- Core Operations
--------------------------------------------------------------------------------
 
type GlobalEnvArg = (?globalEnv :: GlobalEnv)

class Core (el :: Energy -> Type) (ty :: Energy -> Type) | el -> ty, ty -> el where
  app :: (GlobalEnvArg) => el e -> el Kinetic -> el e
  proj :: (GlobalEnvArg) => el e -> QName -> el e
  code :: (GlobalEnvArg) => ty e -> el e
  decode :: (GlobalEnvArg) => Universe -> el e -> ty e
  universe :: (GlobalEnvArg) => Universe -> ty e

instance Core ElS TyS where
  app = App
  proj = Proj
  code = Code
  decode = Decode
  universe = U

appClo :: (Eval a b, GlobalEnvArg) => Clo (a e) (b e) -> ElV Kinetic -> b e
appClo (Clo env _ t) v = evalIn (env :> v) t
appClo (VConst v) _ = v

appTy :: (GlobalEnvArg) => TyV Kinetic -> ElV Kinetic -> TyV Kinetic
appTy a v = case behavesAs a of
  BehavesAs (VPi _ _ cod) -> appClo cod v
  _ -> impossible

projTy :: (GlobalEnvArg) => TyV Kinetic -> QName -> ElV Kinetic -> TyV Kinetic
projTy a x v = case behavesAs a of
  BehavesAs (VRecord _ env fs) -> go env fs
    where
      go _ FNil = impossible
      go env' (FCons x' a' fs')
        | x == x' = evalIn env' a'
        | otherwise = go (env' :> proj v x') fs'
  _ -> impossible

instance Core ElV TyV where
  app (VLam clo) v = appClo clo v
  app (VNeu n) v = VNeu n'
    where
      b' = fmap (flip app v) n.behavesAs
      n' = Neutral n.head (SApp n.spine v) b' (appTy n.ty v)
  app _ _ = impossible

  -- TODO: this is quadratic!! (maybe linear if laziness smiles on me)
  proj (VCons fs) x = elemAt fs x
  proj (VNeu n) x = VNeu n'
    where
      b' = fmap (flip proj x) n.behavesAs
      n' = Neutral n.head (SProj n.spine x) b' (projTy n.ty x (VNeu n))
  proj _ _ = impossible

  code (VDecode _ n) = VNeu n
  code a = VCode a

  decode :: Universe -> ElV e -> TyV e
  decode _ (VCode a) = a
  decode u (VNeu n) = VDecode u n
  decode _ _ = impossible

  universe = VU

behavesAs :: (GlobalEnvArg) => TyV Kinetic -> BehavesAs (TyV Potential)
behavesAs (VU u) = BehavesAs (VU u)
behavesAs (VDecode u n) = case n.behavesAs of
  BehavesAs v -> BehavesAs (decode u v)
  TrueNeutral -> TrueNeutral
behavesAs (VPi pv a b) = BehavesAs (VPi pv a b)

-- Evaluation
--------------------------------------------------------------------------------

type EnvArg = (?env :: Env)

class Eval (a :: Energy -> Type) (b :: Energy -> Type) | a -> b where
  eval :: (EnvArg, GlobalEnvArg) => a e -> b e

evalIn :: (GlobalEnvArg, Eval a b) => Env -> a e -> b e
evalIn env t = let ?env = env in eval t

evalAbs :: (GlobalEnvArg, EnvArg, Eval a b) => Abs (a e) -> Clo (a e) (b e)
evalAbs (Abs x t) = Clo ?env x t
evalAbs (Const t) = VConst (eval t)

instance Eval ElS ElV where
  eval = \case
    Var i -> elemAt ?env i
    GlobalVar c -> case elemAt ?globalEnv c of
      KineticEntry _ v _ -> v
      PotentialEntry _ v a -> VNeu (Neutral (Global c) SId (BehavesAs v) a)
    Code t -> code $ eval t
    App t1 t2 -> eval t1 `app` eval t2
    Lam c -> VLam (evalAbs c)
    Proj t x -> eval t `proj` x
    Cons fs -> VCons $ eval <$> fs

instance Eval TyS TyV where
  eval = \case
    U u -> VU u
    Decode u t -> decode u (eval t)
    Pi pv dom cod -> VPi pv (eval dom) (evalAbs cod)
    Record l fs -> VRecord l ?env fs

-- Quoting
--------------------------------------------------------------------------------

type CtxLenArg = (?ctxLen :: Int)

class Quote (a :: Energy -> Type) (b :: Energy -> Type) | a -> b where
  quote :: (CtxLenArg, GlobalEnvArg) => a -> b

