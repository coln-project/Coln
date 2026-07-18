-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Core.Readback where

import Data.Vector.Strict qualified as Vector
import Prelude hiding (abs)

import Coln.Common
import Coln.Core.Params
import Coln.Core.Syntax qualified as S
import Coln.Core.Value qualified as V

type CtxLen = Int

class Readback a b | a -> b where
  readb :: CtxLen -> a -> b

instance Readback V.Head (S.El N) where
  readb n = \case
    V.LocalVar (FId i) -> S.LocalVar (BId (n - i - 1))
    V.GlobalVar x v -> S.GlobalVar x v
    V.Lookup x vs a -> S.Lookup x (readb n <$> vs) (readb n a)

instance Readback V.Spine (S.El N -> S.El N) where
  readb n = \case
    V.Id -> \t -> t
    V.App sp v -> \t -> S.App (readb n sp t) (readb n v)
    V.Proj sp x -> \t -> S.Proj (readb n sp t) x

instance Readback V.BareNeutral (S.El N) where
  readb n ne = readb n ne.spine $ readb n ne.head

instance Readback (V.Description V.El) (S.El D) where
  readb n = \case
    V.Describe v -> readb n v
    V.Become v -> S.Is (readb n v)

readbClo :: (Readback (V.Evaluation a c) (b c)) => CtxLen -> V.Ty N -> V.Clo a c -> S.Abs b c
readbClo n dom = \case
  V.Clo x l f -> S.Abs x $ readb (n + 1) (f (V.LSnoc l $ V.local (FId n) dom))
  V.CloConst t -> S.AbsConst $ readb n t

instance (V.HasEvaluation c) => Readback (V.El c) (S.El c) where
  readb n = \case
    V.Neu ne -> readb n ne.spine $ readb n ne.head
    V.Code a -> S.Code (readb n a)
    V.Lam dom body -> S.Lam (readb n dom) $ case V.scase @c of
      SNominative -> readbClo n dom body
      SDescriptive -> readbClo n dom body
    V.Cons d -> S.Cons $ case V.scase @c of
      SNominative -> readb n <$> d
      SDescriptive -> readb n <$> d
    V.Init a sp -> readb n sp $ S.Init (readb n a)
    V.Lit l -> S.Lit l

instance Readback V.FunctionType (S.FunctionType S.Ty) where
  readb n f =
    S.FunctionType
      { S.variant = f.variant
      , S.dom = readb n f.dom
      , S.cod = readbClo n f.dom f.cod
      }

instance Readback V.RecordType (S.RecordType S.Ty) where
  readb n r =
    S.RecordType
      { S.level = r.level
      , S.fieldTypes =
          Dict
            { head = r.fieldTypes.head
            , values = Vector.fromList $ go n r.capture r.fieldTypes.values
            }
      }
   where
    go i ls fs =
      if Vector.null fs
        then []
        else do
          let ty = Vector.head fs ls
          readb i ty : go (i + 1) (V.LSnoc ls $ V.local (FId i) ty) (Vector.tail fs)

instance Readback V.EqualityType (S.EqualityType S.El S.Ty) where
  readb n eq =
    S.EqualityType
      { S.at = readb n eq.at
      , S.lhs = readb n eq.lhs
      , S.rhs = readb n eq.rhs
      }

instance (V.HasEvaluation c) => Readback (V.Ty c) (S.Ty c) where
  readb n = \case
    V.U u -> S.U u
    V.Decode ne -> S.Decode $ readb n ne.spine $ readb n ne.head
    V.Function f -> S.Function $ readb n f
    V.Record r -> S.Record $ readb n r
    V.Eq eq -> S.Eq $ readb n eq
    V.BuiltinTy b -> S.BuiltinTy b
    V.EltOf x vs -> S.EltOf x (readb n <$> vs)

instance Readback V.TypeBehavior S.TypeBehavior where
  readb n = \case
    V.LikeU u -> S.LikeU u
    V.LikeRecord rt -> S.LikeRecord $ readb n rt
    V.LikeFunction ft -> S.LikeFunction $ readb n ft
    V.LikeBuiltinTy bt -> S.LikeBuiltinTy bt
    V.NoRules -> S.NoRules
