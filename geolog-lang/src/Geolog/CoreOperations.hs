module Geolog.CoreOperations where

import Control.Monad (forM_, unless)
import Data.Kind (Type)
import Diagnostician
import FNotation (Name)
import Geolog.Common
import Geolog.Core
import Geolog.Pretty
import Prettyprinter

-- Core typeclass
--------------------------------------------------------------------------------

class Core (el :: Energy -> Type) (ty :: Energy -> Type) | el -> ty, ty -> el where
  app :: el e -> el K -> el e
  proj :: el e -> Name -> el e
  code :: ty e -> el e
  decode :: Universe -> el e -> ty e
  universe :: Universe -> ty e
  builtinTy :: BuiltinTy -> ty e
  lit :: Literal -> el K

instance Core ElS TyS where
  app = App
  proj = Proj
  code = Code
  decode = Decode
  universe = U
  builtinTy = BuiltinTy
  lit = Lit

appClo :: Clo (b e) -> ElV K -> b e
appClo (Clo _ f) v = f v
appClo (CloConst v) _ = v

appTy :: TyV Kinetic -> ElV Kinetic -> TyV Kinetic
appTy a v = case behavesAs a of
  Just (VPi _ _ cod) -> appClo cod v
  _ -> panic "appTy should only be called on types that behave like pi types"

projTy :: TyV Kinetic -> Name -> ElV Kinetic -> TyV Kinetic
projTy a x v = case behavesAs a of
  Just (VRecord _ names tys) -> go names tys
    where
      go (x' : xs) (TVCons a' tys')
        | x == x' = a'
        | otherwise = go xs (tys' (proj v x'))
      go _ _ =
        panic "projTy should only be called on fields that exist within the record"
  _ -> panic "projTy should only be called on types that behave like records"

coerceToFields :: ElV e -> Fields (ElV e)
coerceToFields (VCons fs) = fs
coerceToFields (VNeu n) = case n.fields of
  Just fs -> fs
  _ -> panic "a neutral of record type should have been created with a thunk for its fields"
coerceToFields _ = panic "a value of record type should be a neutral or cons"

instance Core ElV TyV where
  app (VLam _ clo) v = appClo clo v
  app (VNeu n) v =
    let a = appTy n.ty v
        behavesAs = app <$> n.behavesAs <*> pure v
     in neu a n.head (SApp n.spine v) behavesAs
  app _ _ = panic "a value of pi type should be a neutral or lam"

  proj v x = elemAt (coerceToFields v) x

  code (VDecode _ n) = VNeu n
  code a = VCode a

  decode :: Universe -> ElV e -> TyV e
  decode _ (VCode a) = a
  decode u (VNeu n) = VDecode u n
  decode _ _ =
    panic "a value of universe type should be a neutral or an encoding of a type"

  universe = VU

  builtinTy = VBuiltinTy
  lit = VLit

behavesAs :: TyV K -> Maybe (TyV P)
behavesAs (VU u) = Just (VU u)
behavesAs (VDecode u n) = decode u <$> n.behavesAs
behavesAs (VPi pv a b) = Just (VPi pv a b)
behavesAs (VBuiltinTy a) = Just $ VBuiltinTy a

expandRecord :: Head -> Spine -> [Name] -> TeleV K -> Fields (ElV K)
expandRecord h sp xs te = Fields xs (go xs te)
  where
    go [] TVNil = []
    go (x : xs') (TVCons a f) =
      let v = neu a h (SProj sp x) Nothing
       in v : go xs' (f v)
    go _ _ = panic "xs and te should have the same length"

neu :: TyV K -> Head -> Spine -> Maybe (ElV P) -> ElV K
neu a h sp be =
  let v = VNeu $ Neutral h sp be fs a
      fs = case behavesAs a of
        Just (VRecord _ xs as) -> Just $ expandRecord h sp xs as
        _ -> Nothing
   in v

local :: TyV K -> FId -> ElV K
local a i = neu a (Local i) SId Nothing

-- Evaluation
--------------------------------------------------------------------------------

type GlobalEnvArg = (?globalEnv :: GlobalEnv)

class Eval a b | a -> b where
  eval :: (GlobalEnvArg) => Env -> a -> b

evalAbs :: (GlobalEnvArg, Eval a b) => Env -> Abs a -> Clo b
evalAbs e (Abs x t) = Clo x (\v -> eval (e :> v) t)
evalAbs e (AbsConst t) = CloConst (eval e t)

instance Eval (ElS e) (ElV e) where
  eval e = \case
    LocalVar i -> elemAt e i
    GlobalVar c -> case elemAt ?globalEnv c of
      KEntry _ v _ -> v
      PEntry _ v a -> neu a (Global c) SId (Just v)
    Code t -> code $ eval e t
    App t1 t2 -> eval e t1 `app` eval e t2
    Lam dom c -> VLam (eval e dom) (evalAbs e c)
    Proj t x -> eval e t `proj` x
    Cons fs -> VCons $ eval e <$> fs
    Lit l -> VLit l

instance Eval (TeleS e) (TeleV e) where
  eval _ TSNil = TVNil
  eval e (TSCons a te) = TVCons (eval e a) (\v -> eval (e :> v) te)

instance Eval (TyS e) (TyV e) where
  eval e = \case
    U u -> VU u
    Decode u t -> decode u (eval e t)
    Pi pv dom cod -> VPi pv (eval e dom) (evalAbs e cod)
    Record l xs te -> VRecord l xs (eval e te)
    BuiltinTy t -> VBuiltinTy t

-- Quoting
--------------------------------------------------------------------------------

type CtxLen = Int

class Quote a b | a -> b where
  quote :: CtxLen -> a -> b

instance Quote FId BId where
  quote n (FId i) = BId (n - i - 1)

instance Quote Head (ElS K) where
  quote n = \case
    Local i -> LocalVar (quote n i)
    Global c -> GlobalVar c

instance Quote Spine (ElS K -> ElS K) where
  quote n sp t = case sp of
    SId -> t
    SApp sp' t' -> App (quote n sp' t) (quote n t')
    SProj sp' x -> Proj (quote n sp' t) x

quoteClo :: (Quote a b) => CtxLen -> TyV K -> Clo a -> Abs b
quoteClo n a (Clo x f) = Abs x $ quote (n + 1) (f (local a (FId n)))
quoteClo n _ (CloConst t) = AbsConst (quote n t)

instance Quote (TeleV K) (TeleS K) where
  quote _ TVNil = TSNil
  quote n (TVCons a f) = TSCons (quote n a) (quote (n + 1) (f (local a (FId n))))

instance Quote (ElV e) (ElS e) where
  quote n = \case
    VNeu (Neutral h sp _ _ _) -> quote n sp (quote n h)
    VCode a -> Code (quote n a)
    VLam dom c -> Lam (quote n dom) (quoteClo n dom c)
    VCons fs -> Cons (quote n <$> fs)
    VLit l -> Lit l

instance Quote (TyV e) (TyS e) where
  quote n = \case
    VU u -> U u
    VDecode u ne -> Decode u (quote n (VNeu ne))
    VPi pv a b -> Pi pv (quote n a) (quoteClo n a b)
    VRecord l xs te -> Record l xs (quote n te)
    VBuiltinTy a -> BuiltinTy a

-- Definitional equality
--------------------------------------------------------------------------------

type CtxShape = MeasuredBwd Name

prtVal :: (Quote a b, Prt b) => CtxShape -> a -> DDoc
prtVal c v = prtTop c.values $ quote c.length v

-- When we do a definitional equality check, how should we report failure?
-- We should report the two things that we were originally looking at (which is
-- not in this code) and the two things which were provably not equal.

-- We only have access to the right bound names at the point in time that we
-- fail to check these things, so we have to pretty print them there.

data DefEqCheckError
  = UnequalTys DDoc DDoc (Maybe DDoc)
  | UnequalEls DDoc DDoc (Maybe DDoc)
  | UnequalSpines Spine Spine (Maybe DDoc)

instance Pretty DefEqCheckError where
  pretty (UnequalTys a a' _) = unAnnotate $ "mismatching types" <+> a <+> "and" <+> a'
  pretty (UnequalEls a a' _) = unAnnotate $ "mismatching elements" <+> a <+> "and" <+> a'
  pretty (UnequalSpines _ _ _) = "can't display unequal spines right now"

type DefEqM a = Either DefEqCheckError a

throwUnequalTys :: CtxShape -> TyV K -> TyV K -> Maybe DDoc -> DefEqM ()
throwUnequalTys cs a a' e =
  Left (UnequalTys (prtVal cs a) (prtVal cs a') e)

throwUnequalEls :: CtxShape -> ElV K -> ElV K -> Maybe DDoc -> DefEqM ()
throwUnequalEls cs v v' e =
  Left (UnequalEls (prtVal cs v) (prtVal cs v') e)

throwUnequalSpines :: Spine -> Spine -> Maybe DDoc -> DefEqM ()
throwUnequalSpines sp sp' e = Left $ UnequalSpines sp sp' e

class DefEq a where
  defEq :: CtxShape -> a -> a -> DefEqM ()

instance DefEq (TyV K) where
  defEq cs a a' = case (a, a') of
    (VU u, VU u') | u == u' -> pure ()
    (VDecode _ n, VDecode _ n') -> defEq cs n n'
    (VPi pv dom cod, VPi pv' dom' cod') -> do
      unless (pv == pv') $
        throwUnequalTys cs a a' $
          Just $
            "different pi variants:" <+> pretty pv <+> "and" <+> pretty pv'
      defEq cs dom dom'
      let v = local dom (FId cs.length)
      defEq (cs ++> "x") (appClo cod v) (appClo cod' v)
    (VBuiltinTy b, VBuiltinTy b') ->
      unless (b == b') $ throwUnequalTys cs a a' $ Just "unequal builtin types"
    _ -> throwUnequalTys cs a a' Nothing

prtHead :: CtxShape -> Head -> DDoc
prtHead cs (Local i) = prtVal cs i
prtHead _ (Global x) = dpretty x

instance DefEq Neutral where
  defEq cs n n' = do
    unless (n.head == n'.head) $
      throwUnequalEls cs (VNeu n) (VNeu n') $
        Just $
          "different heads for neutral:"
            <+> prtHead cs n.head
            <+> "and"
            <+> prtHead cs n'.head
    -- TODO: catch the UnequalSpines error and rethrow it as UnequalEls
    defEq cs n.spine n'.spine

instance DefEq Spine where
  defEq cs sp sp' = case (sp, sp') of
    (SId, SId) -> pure ()
    (SApp sq v, SApp sq' v') -> do
      defEq cs sq sq'
      defEq cs v v'
    (SProj sq x, SProj sq' x') -> do
      defEq cs sq sq'
      unless (x == x') $
        throwUnequalSpines sq sq' $
          Just $
            "fields are not equal:" <+> dpretty x <+> "and" <+> dpretty x'
    _ -> throwUnequalSpines sp sp' Nothing

canon :: ElV K -> ElV K
canon v@(VNeu n) = case behavesAs n.ty of
  Just (VRecord _ _ _) -> VCons (unwrap n.fields)
  Just (VPi _ dom _) -> VLam dom $ Clo "x" $ \w -> app v w
  _ -> v
canon v = v

instance DefEq (ElV K) where
  defEq cs v v' = case (canon v, canon v') of
    (VNeu n, VNeu n') -> defEq cs n n'
    (VCode a, VCode a') -> defEq cs a a'
    (VLam a c, VLam _ c') -> do
      let v = local a (FId cs.length)
      defEq (cs ++> "x") (appClo c v) (appClo c' v)
    (VCons (Fields _ vs), VCons (Fields _ vs')) ->
      forM_ (zip vs vs') (uncurry (defEq cs))
    (VLit l, VLit l') ->
      unless (l == l') $ throwUnequalEls cs v v' $ Just "unequal literals"
    _ -> throwUnequalEls cs v v' Nothing
