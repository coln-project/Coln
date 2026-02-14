module Geolog.Evaluation where

import Data.Kind (Type)
import Geolog.Common
import Geolog.Core

-- Core Operations
--------------------------------------------------------------------------------

type GlobalEnvArg = (?globalEnv :: GlobalEnv)

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

appTy :: TyV Kinetic -> ElV Kinetic -> TyV Kinetic
appTy a v = case behavesAs a of
  BehavesAs (VPi _ _ cod) -> appClo cod v
  _ -> impossible

projTy :: TyV Kinetic -> QName -> ElV Kinetic -> TyV Kinetic
projTy a x v = case behavesAs a of
  BehavesAs (VRecord _ (FieldsV names tys)) -> go names tys
   where
    go [] _ = impossible
    go (x' : xs) (TVCons a' tys')
      | x == x' = a'
      | otherwise = go xs (tys' (proj v x'))
    go _ _ = impossible
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

teleEq :: ElV K -> ElV K -> [QName] -> TeleV (TyV K) -> TeleV (TyV K) -> TeleV (TyV K)
teleEq _ _ _ TVNil TVNil = TVNil
teleEq v v' (x : xs) (TVCons a fs) (TVCons a' fs') =
  let vx = proj v x
      vx' = proj v' x
      fs'' = teleEq v v' xs (fs vx) (fs' vx')
   in TVCons (VTmEq (proj v x) a (proj v' x) a') (\_ -> fs'')
teleEq _ _ _ _ _ = impossible

behavesAs :: TyV K -> BehavesAs (TyV Potential)
behavesAs (VU u) = BehavesAs (VU u)
behavesAs (VDecode u n) = case n.behavesAs of
  BehavesAs v -> BehavesAs (decode u v)
  TrueNeutral -> TrueNeutral
behavesAs (VPi pv a b) = BehavesAs (VPi pv a b)
behavesAs (VTyEq _ _) = TrueNeutral
behavesAs (VTmEq v a v' a') = case (behavesAs a, behavesAs a') of
  (BehavesAs b, BehavesAs b') -> case (b, b') of
    (VPi pv dom (Clo x f), VPi pv' dom' (Clo x' f')) ->
      if pv == pv'
        then
          BehavesAs $
            VPi
              pv
              dom
              ( Clo x $ \vx ->
                  VPi
                    pv
                    dom'
                    ( Clo x' $ \vx' ->
                        VPi
                          pv
                          (VTmEq vx dom vx' dom')
                          ( Clo "pf" $ \_ ->
                              VTmEq (app v vx) (f vx) (app v' vx') (f' vx')
                          )
                    )
              )
        else TrueNeutral
    (VRecord l (FieldsV names tele), VRecord l' (FieldsV names' tele')) ->
      if l == l' && names == names'
        then BehavesAs $ VRecord l (FieldsV names (teleEq v v' names tele tele'))
        else TrueNeutral
    _ -> TrueNeutral
  _ -> TrueNeutral

-- Evaluation
--------------------------------------------------------------------------------

type EnvArg = (?env :: Env)

class Eval (a :: Energy -> Type) (b :: Energy -> Type) | a -> b where
  eval :: (EnvArg, GlobalEnvArg) => a e -> b e

evalIn :: (GlobalEnvArg, Eval a b) => Env -> a e -> b e
evalIn env t = let ?env = env in eval t

evalAbs :: (GlobalEnvArg, EnvArg, Eval a b) => Abs (a e) -> Clo (b e)
evalAbs (Abs x t) = Clo x (\v -> evalIn (?env :> v) t)
evalAbs (Const t) = let v = eval t in Clo "x" (\_ -> v)

coerceCons :: [QName] -> TeleV (TyV K) -> TeleV (TyV K) -> ElV K -> Fields (ElV K)
coerceCons [] _ _ _ = FNil
coerceCons (x : xs) (TVCons a f) (TVCons a' f') v =
  let vx = proj v x
      vx' = coerce (proj v x) a a'
   in FCons x vx' (coerceCons xs (f vx) (f' vx') v)
coerceCons _ _ _ _ = impossible

coerce :: ElV K -> TyV K -> TyV K -> ElV K
coerce v a a' = case (behavesAs a, behavesAs a') of
  (BehavesAs b, BehavesAs b') -> case (b, b') of
    (VU u, VU u') | u == u' -> v
    (VPi pv dom (Clo _ f), VPi pv' dom' (Clo _ f'))
      | pv == pv' ->
          VLam $
            Clo
              "x"
              ( \vx' ->
                  let vx = coerce vx' dom' dom
                   in coerce (app v vx) (f vx) (f' vx')
              )
    (VRecord l (FieldsV xs te), VRecord l' (FieldsV xs' te'))
      | l == l' && xs == xs' -> VCons (coerceCons xs te te' v)
    _ -> VNeu $ Neutral (VCoe v a a') SId TrueNeutral a'
  (TrueNeutral, TrueNeutral) | isConv a a' -> v
  _ -> VNeu $ Neutral (VCoe v a a') SId TrueNeutral a'

instance Eval ElS ElV where
  eval = \case
    Var i -> elemAt ?env i
    GlobalVar c -> case elemAt ?globalEnv c of
      KEntry _ v _ -> v
      PEntry _ v a -> VNeu (Neutral (Global c) SId (BehavesAs v) a)
    Code t -> code $ eval t
    App t1 t2 -> eval t1 `app` eval t2
    Lam c -> VLam (evalAbs c)
    Proj t x -> eval t `proj` x
    Cons fs -> VCons $ eval <$> fs
    TyRefl _ -> VIrrel
    TmRefl _ _ -> VIrrel
    Subst _ _ _ _ _ -> VIrrel
    Coh _ _ _ _ -> VIrrel
    Irrel -> VIrrel
    Coerce a t b _ -> coerce (eval t) (eval a) (eval b)

evalFields :: (GlobalEnvArg, EnvArg) => Fields (TyS e) -> TeleV (TyV e)
evalFields FNil = TVNil
evalFields (FCons _ a fs) = TVCons (eval a) (\v -> let ?env = ?env :> v in evalFields fs)

fieldNames :: Fields a -> [QName]
fieldNames FNil = []
fieldNames (FCons x _ fs) = x : fieldNames fs

instance Eval TyS TyV where
  eval = \case
    U u -> VU u
    Decode u t -> decode u (eval t)
    Pi pv dom cod -> VPi pv (eval dom) (evalAbs cod)
    Record l fs -> VRecord l (FieldsV (fieldNames fs) (evalFields fs))
    TyEq a b -> VTyEq (eval a) (eval b)
    TmEq t a t' a' -> VTmEq (eval t) (eval a) (eval t') (eval a')

-- Quoting
--------------------------------------------------------------------------------

type CtxLenArg = (?ctxLen :: Int)

class Quote (a :: Energy -> Type) (b :: Energy -> Type) | a -> b where
  quote :: (CtxLenArg, GlobalEnvArg) => a e -> b e

-- Conversion
--------------------------------------------------------------------------------

isConv :: TyV K -> TyV K -> Bool
isConv = unimplemented
