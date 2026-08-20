module Coln.MIR.Readback where

import Coln.Common
import Coln.Core.Params
import Coln.MIR.Params
import Coln.MIR.Syntax qualified as S
import Coln.MIR.Value qualified as V

type CtxLen = Int

class Readback a b | a -> b where
  readb :: CtxLen -> a -> b

instance Readback V.Head (S.El Set) where
  readb n = \case
    V.Var (FId i) -> S.Var (BId (n - i - 1))
    V.Lookup x args a -> S.Lookup x (readb n <$> args) (readb n a)

instance Readback (V.El Set) (S.El Set) where
  readb n = \case
    V.Neu ne -> do
      let go t BwdNil = t
          go t (xs :> x) = S.Proj (go t xs) x
      go (readb n ne.head) ne.spine
    V.Cons fields -> S.Cons $ readb n <$> fields
    V.Lit l -> S.Lit l

fresh :: CtxLen -> V.El Set
fresh n = V.local (FId n)

instance Readback (V.Ty Set) (S.Ty Set) where
  readb n = \case
    V.EltOf tn args -> S.EltOf tn (readb n <$> args)
    V.BuiltinTy t -> S.BuiltinTy t
    V.Eq at lhs rhs -> S.Eq (readb n at) (readb n lhs) (readb n rhs)
    V.Record rt -> do
      let go _ _ [] = []
          go n' vs ((x, k) : rest) =
            (x, readb n' (k vs)) : (go (n' + 1) (vs :> Pair SSet (fresh n')) rest)
      let fieldTypes = fromList $ go n rt.capture (toList rt.fieldTypes)
      S.Record $ S.RecordType fieldTypes

instance Readback (V.Ty Theory) (S.Ty Theory) where
  readb n = \case
    V.LiftTy LSetTheory a -> S.LiftTy (readb n a)
    V.U SPropU -> S.U SPropU
    V.U SSetU -> S.U SSetU
    V.Function ft -> undefined

readbClo :: (Readback a b) => CtxLen -> V.Clo (V.El Set) a -> S.Abs b
readbClo n (V.Clo x f) = S.Abs x (readb (n + 1) (f (fresh n)))
readbClo n (V.CloConst t) = S.AbsConst (readb n t)

instance Readback (V.El Theory) (S.El Theory) where
  readb n = \case
    V.LiftEl LSetTheory v -> S.LiftEl (readb n v)
    V.Code SPropU a -> S.Code SPropU (readb n a)
    V.Code SSetU a -> S.Code SSetU (readb n a)
    V.Lam SSetTheory dom clo -> S.Lam (readb n dom) (readbClo n clo)
    V.Cons fields -> S.Cons $ readb n <$> fields
