module Coln.SIR.Separate where

import Coln.Common
import Coln.Core.Params
import Coln.MIR.Params
import Coln.MIR.Value qualified as V
import Coln.SIR.Syntax qualified as S

type CtxLen = Int

class Separate a b | a -> b where
  separate :: CtxLen -> a -> b

instance Separate V.Head (S.El Set) where
  separate n = \case
    V.Var (FId i) -> S.Var (BId (n - i - 1))
    V.Lookup tn args ret -> do
      let args' = separate n <$> args
      let pred = S.Atom tn Nothing (args' ++ [S.Var 0])
      S.Single $ S.Query (shapeOf ret) (S.Abs Nothing pred)

instance Separate (V.El Set) (S.El Set) where
  separate n = \case
    V.Neu ne -> do
      let go t BwdNil = t
          go t (xs :> x) = S.Proj (go t xs) x
      go (separate n ne.head) ne.spine
    V.Cons fields -> S.Cons $ separate n <$> fields
    V.Lit l -> S.Lit l

separateClo :: (Separate a b) => CtxLen -> V.Clo (V.El Set) a -> S.Abs b
separateClo n (V.Clo x body) = S.Abs (Just x) (separate (n + 1) (body (V.local (FId n))))
separateClo n (V.CloConst body) = S.AbsConst (separate n body)

instance Separate (V.El Theory) (S.El Theory) where
  separate n = \case
    V.LiftEl LSetTheory v -> S.LiftEl (separate n v)
    V.Code SSetU a -> S.Multi SSetU (separate n a)
    V.Code SPropU a -> S.Multi SPropU (separate n a)
    V.Lam SSetTheory dom clo -> S.Lam (separate n dom) (separateClo n clo)
    V.Cons fields -> S.Cons $ separate n <$> fields

shapeOf :: V.Ty Set -> S.Shape
shapeOf = \case
  V.EltOf x _ -> S.RowId x
  V.Record rt -> do
    let go [] _ _ = []
        go ((x, k):rest) vs v = do
          let v' = V.proj v x
          (x, shapeOf (k vs)) : (go rest (vs :> Pair SSet v') v)
    let v = V.local (FId 0)
    S.Tuple $ fromList $ go (toList rt.fieldTypes) rt.capture v
  V.BuiltinTy t -> S.BuiltinTy t
  V.Eq _ _ _ -> S.unitShape

propAt :: CtxLen -> V.Ty Set -> V.El Set -> S.Prop
propAt n = \case
  V.EltOf x args -> \v ->
    S.Atom x (Just (separate n v)) (separate n <$> args)
  V.Record rt -> \v -> do
    let go [] _ = []
        go ((x, k):rest) vs = do
          let v' = V.proj v x
          (x, propAt n (k vs) v') : (go rest (vs :> Pair SSet v'))
    S.And $ fromList $ go (toList rt.fieldTypes) rt.capture
  V.BuiltinTy _ -> \_ -> S.trueProp
  V.Eq at lhs rhs -> \_ ->
    S.Eq (shapeOf at) (separate n lhs) (separate n rhs)

instance Separate (V.Ty Set) S.Query where
  separate n a =
    S.Query (shapeOf a) (S.Abs Nothing (propAt (n + 1) a (V.local (FId n))))
