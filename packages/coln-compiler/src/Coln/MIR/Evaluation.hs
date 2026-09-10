module Coln.MIR.Evaluation where

import Coln.Common
import Coln.Core.Params
import Coln.MIR.Params

import Coln.MIR.Syntax qualified as S
import Coln.MIR.Value qualified as V

class Eval (a :: Case -> MLevel -> Type) (b :: Case -> MLevel -> Type) where
  eval :: (V.HasEvaluation c) => V.Locals -> a c l -> V.Evaluation b c l

evalAbs :: (V.HasEvaluation c, Eval a b) => V.Locals -> S.Abs (a c l) -> V.Clo (V.El N Set) (V.Evaluation b c l)
evalAbs vs (S.Abs x body) = V.Clo x (\v -> eval (vs :> Pair SSet v) body)
evalAbs vs (S.AbsConst body) = V.CloConst (eval vs body)

instance Eval S.El V.El where
  eval vs = \case
    S.LiftEl t -> V.emap (V.LiftEl LSetTheory) (eval vs t)
    S.Var i -> levelCoerceFromMatch SSet (elemAt vs i)
    S.Lookup tn args a ->
      V.Neu $ V.Neutral (V.Lookup tn (eval vs <$> args) (eval vs a)) BwdNil
    S.Code u a -> V.Code u (eval vs a)
    S.PrimCode u tn args -> V.PrimCode u tn (eval vs <$> args)
    S.Lam dom abs -> V.epure $ V.Lam SSetTheory (eval vs dom) (evalAbs vs abs)
    S.Cons fields -> V.epure $ V.Cons (eval vs <$> fields)
    S.Proj t x -> V.proj (eval vs t) x
    S.Lit l -> V.Lit l
    S.Erased -> V.Erased
    S.Is t -> V.Become (eval vs t)

instance Eval S.Ty V.Ty where
  eval vs = \case
    S.LiftTy t -> V.emap (V.LiftTy LSetTheory) (eval vs t)
    S.U u -> V.U u
    S.EltOf u tn args -> V.EltOf u tn (eval vs <$> args)
    S.Function ft ->
      V.Function $
        V.FunctionType ft.variant (eval vs ft.dom) (evalAbs vs ft.cod)
    S.Record rt ->
      V.Record $
        V.RecordType rt.hlevel vs $
          flip eval <$> rt.fieldTypes
    S.BuiltinTy t -> V.BuiltinTy t
    S.Eq at lhs rhs -> V.Eq (eval vs at) (eval vs lhs) (eval vs rhs)
