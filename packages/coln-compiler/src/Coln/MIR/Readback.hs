module Coln.MIR.Readback where

import Coln.Common
import Coln.Core.Params
import Coln.MIR.Params
import Coln.MIR.Syntax qualified as S
import Coln.MIR.Value qualified as V

-- import Data.Traversable (mapAccumL)

type CtxLen = Int

class Readback a b | a -> b where
  readb :: CtxLen -> a -> b

instance Readback V.Head (S.El N Set) where
  readb n = \case
    V.Var (FId i) -> S.Var (BId (n - i - 1))
    V.Lookup x args a -> S.Lookup x (readb n <$> args) (readb n a)

instance Readback (V.El N Set) (S.El N Set) where
  readb n = \case
    V.Neu ne -> do
      let go t BwdNil = t
          go t (xs :> x) = S.Proj (go t xs) x
      go (readb n ne.head) ne.spine
    V.Cons fields -> S.Cons $ readb n <$> fields
    V.Lit l -> S.Lit l
    V.Erased -> S.Erased

fresh :: CtxLen -> V.El N Set
fresh n = V.local (FId n)

instance Readback (V.Ty N Set) (S.Ty N Set) where
  readb n = \case
    V.EltOf u tn args -> S.EltOf u tn (readb n <$> args)
    V.BuiltinTy t -> S.BuiltinTy t
    V.Eq at lhs rhs -> S.Eq (readb n at) (readb n lhs) (readb n rhs)
    V.Record rt -> do
      let go _ _ [] = []
          go n' vs ((x, k) : rest) =
            (x, readb n' (k vs)) : (go (n' + 1) (vs :> Pair SSet (fresh n')) rest)
      let fieldTypes = fromList $ go n rt.capture (toList rt.fieldTypes)
      S.Record $ S.RecordType rt.hlevel fieldTypes

-- instance Readback (V.Ty N Theory) (S.Ty N Theory) where
--   readb n = \case
--     V.LiftTy LSetTheory a -> S.LiftTy (readb n a)
--     V.U SPropU -> S.U SPropU
--     V.U SSetU -> S.U SSetU
--     V.Function ft -> undefined
--     V.Function ft -> case ft.variant.mlevel of
--       SSetTheory -> S.Function (S.FunctionType ft.variant (readb n ft.dom) (readbClo n ft.cod))
--     V.Record rt -> S.Record (S.RecordType rt.hlevel (readbTele n rt.capture rt.fieldTypes))
--
-- readbTele :: (Traversable f, Readback a b) => CtxLen -> V.Locals -> f (V.Locals -> a) -> f b
-- readbTele n l = snd . mapAccumL (\(n', l') k -> ((n' + 1, l' :> Pair SSet (V.local (FId n'))), readb n' $ k l')) (n, l)

readbClo :: (Readback a b) => CtxLen -> V.Clo (V.El N Set) a -> S.Abs b
readbClo n (V.Clo x f) = S.Abs x (readb (n + 1) (f (fresh n)))
readbClo n (V.CloConst t) = S.AbsConst (readb n t)

instance Readback (V.El N Theory) (S.El N Theory) where
  readb n = \case
    V.LiftEl LSetTheory v -> S.LiftEl (readb n v)
    V.Code SPropU a -> S.Code SPropU (readb n a)
    V.Code SSetU a -> S.Code SSetU (readb n a)
    V.PrimCode u tn args -> S.PrimCode u tn (readb n <$> args)
    V.Lam SSetTheory dom clo -> S.Lam (readb n dom) (readbClo n clo)
    V.Cons fields -> S.Cons $ readb n <$> fields
