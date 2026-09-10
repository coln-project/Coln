-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT
{-# LANGUAGE TypeAbstractions #-}

module Coln.Core.Memoed where

import Coln.Common
import Coln.Core.Evaluation
import Coln.Core.Params
import Coln.Core.Readback (Readback (readb))
import Coln.Core.Syntax qualified as S
import Coln.Core.Value qualified as V

data Memoed stx val c = M
  { stx :: stx c
  , val :: ~(V.Evaluation val c)
  }

type El = Memoed S.El V.El

type Ty = Memoed S.Ty V.Ty

class Core el ty | el -> ty, ty -> el where
  localVar :: BId -> V.El N -> el N
  globalVar :: Name -> V.El N -> el N
  code :: (V.HasEvaluation c) => Universe -> ty c -> el c
  app :: FunctionVariant -> el N -> el N -> el N
  lam :: (V.HasEvaluation c) => FunctionVariant -> V.Locals -> ty N -> S.Abs el c -> el c
  cons :: (V.HasEvaluation c) => Level -> Dict (el c) -> el c
  proj :: Level -> el N -> Name -> el N
  init :: ty N -> el D
  lit :: Literal -> el N
  is :: el N -> el D
  univ :: Universe -> ty N
  decode :: Universe -> el N -> ty N
  function :: V.Locals -> FunctionVariant -> ty N -> S.Abs ty N -> ty N
  record :: V.Locals -> S.RecordType ty -> ty D
  equality :: S.EqualityType el ty -> ty N
  builtinTy :: BuiltinTy -> ty N
  isTy :: ty N -> ty D

instance Core El Ty where
  localVar i v = M (S.LocalVar i) v
  globalVar x v = M (S.GlobalVar x v) v
  code u t = M (S.Code u t.stx) (V.emap (V.Code u) t.val)
  app fv f x = M (S.App fv f.stx x.stx) (V.app fv f.val x.val)
  lam fv vs dom (S.Abs x body) =
    M
      (S.Lam fv dom.stx (S.Abs x body.stx))
      (V.epure $ V.Lam fv dom.val (V.Clo x vs (compile body.stx)))
  lam fv _ dom (S.AbsConst body) =
    M
      (S.Lam fv dom.stx (S.AbsConst body.stx))
      (V.epure $ V.Lam fv dom.val (V.CloConst body.val))
  cons l d = M (S.Cons l $ (.stx) <$> d) (V.epure $ V.Cons l $ (.val) <$> d)
  proj l x f = M (S.Proj l x.stx f) (V.proj x.val f)
  init a =
    M (S.Init a.stx) (V.BecomeWith $ \n -> V.InitNeu (V.InitNeutral n a.val V.Id))
  lit l = M (S.Lit l) (V.Lit l)
  is x = M (S.Is x.stx) (V.Become x.val)
  univ u = M (S.U u) (V.U u)
  decode u x = M (S.Decode u x.stx) (V.decode x.val)
  function vs fv dom (S.Abs x body) =
    M
      (S.Function $ S.FunctionType fv dom.stx (S.Abs x body.stx))
      (V.Function $ V.FunctionType fv dom.val (V.Clo x vs (compile body.stx)))
  function _ fv dom (S.AbsConst body) =
    M
      (S.Function $ S.FunctionType fv dom.stx (S.AbsConst body.stx))
      (V.Function $ V.FunctionType fv dom.val (V.CloConst body.val))
  record vs rt =
    M
      (S.Record $ S.RecordType rt.level $ (.stx) <$> rt.fieldTypes)
      (V.epure $ V.Record $ V.RecordType rt.level vs $ compile . (.stx) <$> rt.fieldTypes)
  equality eq =
    M
      (S.Eq $ S.EqualityType eq.at.stx eq.lhs.stx eq.rhs.stx)
      (V.Eq $ V.EqualityType eq.at.val eq.lhs.val eq.rhs.val)
  builtinTy bt = M (S.BuiltinTy bt) (V.BuiltinTy bt)
  isTy a = M (S.IsTy a.stx) (V.Become a.val)

fromVTy :: (V.HasEvaluation c) => Int -> V.Ty c -> Ty c
fromVTy n v = M (readb n v) (V.epure v)

fromVEl :: (V.HasEvaluation c) => Int -> V.El c -> El c
fromVEl n v = M (readb n v) (V.epure v)

instance (V.HasEvaluation c) => LevelOf (Ty c) where
  levelOf ty = case V.scase @c of
    SNominative -> levelOf ty.val
    SDescriptive -> case ty.val of
      V.Describe ty' -> levelOf ty'
      V.Become ty' -> levelOf ty'
      -- This is kind of a hack, but shouldn't appear in practice in any case
      V.BecomeWith f -> levelOf (f $ V.BareNeutral (V.LocalVar (FId 0)) V.Id)

instance Readback (Memoed a b c) (a c) where
  readb _ m = m.stx
