-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT
{-# LANGUAGE TypeAbstractions #-}

module Coln.Core.Memoed where

import Data.Coerce (coerce)

import Coln.Common
import Coln.Core.Evaluation
import Coln.Core.Globals
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
  code :: (V.HasEvaluation c) => ty c -> el c
  app :: el N -> el N -> el N
  lam :: (V.HasEvaluation c) => V.Locals -> ty N -> S.Abs el c -> el c
  cons :: (V.HasEvaluation c) => Dict (el c) -> el c
  proj :: el N -> Name -> el N
  lit :: Literal -> el N
  is :: el N -> el D
  univ :: Universe -> ty N
  decode :: el N -> ty N
  function :: V.Locals -> FunctionVariant -> ty N -> S.Abs ty N -> ty N
  record :: V.Locals -> S.RecordType ty -> ty D
  equality :: S.EqualityType el ty -> ty N
  builtinTy :: BuiltinTy -> ty N
  isTy :: ty N -> ty D

instance Core El Ty where
  localVar i v = M (S.LocalVar i) v
  globalVar x v = M (S.GlobalVar x v) v
  code t = M (S.Code t.stx) (V.emap V.Code t.val)
  app f x = M (S.App f.stx x.stx) (V.app f.val x.val)
  lam vs dom (S.Abs x body) =
    M
      (S.Lam dom.stx (S.Abs x body.stx))
      (V.epure $ V.Lam dom.val (V.Clo x vs (compile body.stx)))
  lam _ dom (S.AbsConst body) =
    M
      (S.Lam dom.stx (S.AbsConst body.stx))
      (V.epure $ V.Lam dom.val (V.CloConst body.val))
  cons d = M (S.Cons $ (.stx) <$> d) (V.epure $ V.Cons $ (.val) <$> d)
  proj x f = M (S.Proj x.stx f) (V.proj x.val f)
  lit l = M (S.Lit l) (V.Lit l)
  is x = M (S.Is x.stx) (V.Become x.val)
  univ u = M (S.U u) (V.U u)
  decode x = M (S.Decode x.stx) (V.decode x.val)
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

instance Readback (Memoed a b c) (a c) where
  readb _ m = m.stx

mkGlobal :: Name -> V.Ty N -> El D -> GlobalEntry
mkGlobal n ty x = do
  let neu = V.reflect (V.GlobalVar n neu) V.Id ty (Just x.val)
  GlobalEntry x.stx neu ty
