module Coln.MIR.Interpret where

-- Interpret Core syntax into MIR values
import Coln.Common

import Coln.MIR.Value qualified as V
import Coln.MIR.Params
import Coln.Core.Syntax qualified as S
import Coln.Core.Params

class Interp a (f :: MLevel -> Type) | a -> f where
  interp :: V.Globals -> V.Locals -> a -> Match SMLevel f

interpAt :: (Interp a f, LevelCoerce f) => SMLevel l -> V.Globals -> V.Locals -> a -> f l
interpAt l0 g e t = case interp g e t of
  Pair l1 v -> levelCoerce l1 l0 v

-- Should this also be "compile"?

instance Interp (S.El c) V.El where
  interp g e = \case
    S.LocalVar i -> elemAt e i
    S.GlobalVar x _ -> elemAt g x
    S.Code u a -> withUniverse u $ \su -> do
      let (l0, l1) = (sDecodesInto su, sCodesInto su)
      Pair l1 (V.Code su (interpAt l0 g e a))
    S.Lam fv _ abs -> withFunctionVariant fv.mlevel $ \sfv -> do
      let (d, c) = (sDom sfv, sCod sfv)
      let clo = case abs of
            S.Abs x body -> V.Clo x (\v -> interpAt c g (e :> Pair d v) body)
            S.AbsConst body -> V.CloConst (interpAt c g e body)
      Pair c (V.Lam sfv clo)
    S.App fv t0 t1 -> withFunctionVariant fv.mlevel $ \sfv -> do
      let (d, c) = (sDom sfv, sCod sfv)
      Pair c (V.app sfv (interpAt c g e t0) (interpAt d g e t1))
    S.Cons l fields -> withLevel l.mlevel $ \sl -> do
      let fields' = interpAt sl g e <$> fields
      Pair sl (V.Cons fields')
    S.Proj l t0 x -> withLevel l.mlevel $ \sl -> do
      let v = interpAt sl g e t0
      Pair sl (V.proj v x)
    S.Init _ -> panic "cannot interpret init yet"
    S.Lit l -> Pair SSet (V.Lit l)
    S.Is t -> interp g e t

instance Interp (S.Ty c) V.Ty where
  interp g e = \case
    S.U u -> withUniverse u $ \su -> Pair (sCodesInto su) (V.U su)
    S.Decode u t -> withUniverse u $ \su ->
      Pair (sDecodesInto su) (V.decode su (interpAt (sCodesInto su) g e t))
    S.Function ft -> withFunctionVariant ft.variant.mlevel $ \sfv -> do
      let (d, c) = (sDom sfv, sCod sfv)
      let dom = interpAt d g e ft.dom
      let cod = case ft.cod of
            S.Abs x body -> V.Clo x (\v -> interpAt c g (e :> Pair d v) body)
            S.AbsConst body -> V.CloConst (interpAt c g e body)
      Pair c (V.Function (V.FunctionType sfv dom cod))
    S.Record rt -> withLevel rt.level.mlevel $ \sl -> do
      let rt' = V.RecordType e (flip (interpAt sl g) <$> rt.fieldTypes)
      Pair sl (V.Record rt')
    S.Eq et -> do
      let at = interpAt SSet g e et.at
      let (lhs, rhs) = (interpAt SSet g e et.lhs, interpAt SSet g e et.rhs)
      Pair SSet $ V.Eq at lhs rhs
    S.BuiltinTy t -> do
      Pair SSet $ V.BuiltinTy t
    S.IsTy t -> interp g e t
