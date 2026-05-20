module Geolog.Core.Evaluation where

import Prelude hiding (abs)
import Data.Vector.Strict qualified as Vector

import Geolog.Common
import Geolog.Core.Params
import Geolog.Core.Syntax qualified as S
import Geolog.Core.Value qualified as V

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
  \vs -> V.CloConst (k vs)

instance Compile S.El V.El where
  compile = \case
    S.LocalVar i -> \vs -> elemAt vs i
    S.GlobalVar _ v -> \_ -> v
    S.Code a -> V.emap V.Code . compile a
    S.App t0 t1 -> do
      let k0 = compile t0
      let k1 = compile t1
      \vs -> V.ebind (`V.app` (k1 vs)) (k0 vs)
    S.Lam dom abs -> do
      let k_dom = compile dom
      let k_clo = compileAbs abs
      \vs -> V.epure $ V.Lam (k_dom vs) (k_clo vs)
    S.Cons fields -> do
      let k_fields = compile <$> fields
      \vs -> V.epure $ V.Cons $ ($ vs) <$> k_fields
    S.Proj t x -> do
      let k = compile t
      \vs -> V.ebind (`V.proj` x) (k vs)
    S.Lit l -> \_ -> V.Lit l
    S.Is t -> do
      let k = compile t
      \vs -> V.Become (k vs)

compileFunctionType :: S.FunctionType S.Ty -> V.Locals -> V.FunctionType
compileFunctionType ft = do
  let k_dom = compile ft.dom
  let k_cod = compileAbs ft.cod
  \vs -> V.FunctionType ft.variant (k_dom vs) (k_cod vs)

compileRecordType :: S.RecordType S.Ty -> V.Locals -> V.RecordType
compileRecordType rt = do
  let k_fieldTypes = compile <$> rt.fieldTypes
  \vs -> V.RecordType rt.level vs k_fieldTypes

instance Compile S.Ty V.Ty where
  compile = \case
    S.U u -> \_ -> V.U u
    S.Decode t -> do
      let k = compile t
      \vs -> V.ebind V.decode (k vs)
    S.Function ft -> V.Function . compileFunctionType ft
    S.Record rt -> V.Describe . V.Record . compileRecordType rt
      
      
