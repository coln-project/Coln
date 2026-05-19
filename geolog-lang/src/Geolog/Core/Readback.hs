module Geolog.Core.Readback where

import Prelude hiding (abs)
import Data.Vector.Strict qualified as Vector

import Geolog.Common
import Geolog.Core.Params
import Geolog.Core.Syntax qualified as S
import Geolog.Core.Value qualified as V

type CtxLen = Int

class Readback a b | a -> b where
  readb :: CtxLen -> a -> b

instance Readback V.Head (S.El N) where
  readb n = \case
    V.LocalVar (FId i) -> S.LocalVar (BId (n - i - 1))
    V.GlobalVar x v -> S.GlobalVar x v

instance Readback V.Spine (S.El N -> S.El N) where
  readb n = \case
    V.Id -> \t -> t
    V.App sp v -> \t -> S.App (readb n sp t) (readb n v)
    V.Proj sp x -> \t -> S.Proj (readb n sp t) x

instance Readback (V.Description V.El) (S.El D) where
  readb n = \case
    V.Describe v -> readb n v
    V.Become v -> S.Is (readb n v)

readbClo :: (Readback (V.Evaluation a c) (b c)) => CtxLen -> V.Clo a c -> S.Abs b c
readbClo = undefined

instance (V.HasEvaluation c) => Readback (V.El c) (S.El c) where
  readb n = \case
    V.Neu ne -> readb n ne.spine $ readb n ne.head
    V.Code a -> S.Code (readb n a)
    V.Lam a body -> S.Lam (readb n a) (readbClo n body)
    V.Cons d -> case V.scase @c of
      SNominative -> S.Cons $ readb n <$> d
      SDescriptive -> S.Cons $ readb n <$> d

instance (V.HasEvaluation c) => Readback (V.Ty c) (S.Ty c) where
  readb n = undefined
