-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Core.Evaluation where

import Coln.Common
import Coln.Core.Params
import Coln.Core.Syntax qualified as S
import Coln.Core.Value qualified as V
import Prelude hiding (abs)

class Compile (a :: Case -> Type) (b :: Case -> Type) | a -> b where
  compile :: (V.HasEvaluation c) => a c -> V.Locals -> V.Evaluation b c

eval :: (V.HasEvaluation c, Compile a b) => V.Locals -> a c -> V.Evaluation b c
eval = flip compile

compileAbs :: (V.HasEvaluation c, Compile a b) => S.Abs a c -> V.Locals -> V.Clo b c
compileAbs (S.Abs x t) = do
  let k = compile t
  \vs -> V.Clo x vs k
compileAbs (S.AbsConst t) = do
  let k = compile t
  V.CloConst . k

instance Compile S.El V.El where
  compile = \case
    S.LocalVar i -> (`elemAt` i)
    S.GlobalVar _ v -> const v
    S.Code a -> V.emap V.Code . compile a
    S.App t0 t1 -> do
      let k0 = compile t0
      let k1 = compile t1
      \vs -> V.app (k0 vs) (k1 vs)
    S.Lam dom abs -> do
      let k_dom = compile dom
      let k_clo = compileAbs abs
      \vs -> V.epure $ V.Lam (k_dom vs) (k_clo vs)
    S.Cons fields -> do
      let k_fields = compile <$> fields
      \vs -> V.epure $ V.Cons $ ($ vs) <$> k_fields
    S.Proj t x -> do
      let k = compile t
      \vs -> V.proj (k vs) x
    S.Lit l -> \_ -> V.Lit l
    S.Is t -> do
      let k = compile t
      V.Become . k
    S.Lookup x ts a -> do
      let kts = compile <$> ts
      let ka = compile a
      \vs -> V.tableLookup x (fmap (\kt -> kt vs) kts) (ka vs)

compileFunctionType :: S.FunctionType S.Ty -> V.Locals -> V.FunctionType
compileFunctionType ft = do
  let k_dom = compile ft.dom
  let k_cod = compileAbs ft.cod
  \vs -> V.FunctionType ft.variant (k_dom vs) (k_cod vs)

compileRecordType :: S.RecordType S.Ty -> V.Locals -> V.RecordType
compileRecordType rt = do
  let k_fieldTypes = compile <$> rt.fieldTypes
  \vs -> V.RecordType rt.level vs k_fieldTypes

compileEqualityType :: S.EqualityType S.El S.Ty -> V.Locals -> V.EqualityType
compileEqualityType eq = do
  let k_at = compile eq.at
  let k_lhs = compile eq.lhs
  let k_rhs = compile eq.rhs
  \vs -> V.EqualityType (k_at vs) (k_lhs vs) (k_rhs vs)

instance Compile S.Ty V.Ty where
  compile = \case
    S.U u -> const $ V.U u
    S.Decode t -> do
      let k = compile t
      V.ebind V.decode . k
    S.Function ft -> V.Function . compileFunctionType ft
    S.Record rt -> V.Describe . V.Record . compileRecordType rt
    S.Eq eq -> V.Eq . compileEqualityType eq
    S.BuiltinTy bt -> \_ -> V.BuiltinTy bt
    S.IsTy a -> do
      let k = compile a
      V.Become . k
    S.EltOf x ts -> do
      let k = compile <$> ts
      \vs -> V.EltOf x $ ($ vs) <$> k
