module Coln.MIR.Memoed where

import Coln.Common
import Coln.Core.Params
import Coln.MIR.Evaluation
import Coln.MIR.Params
import Coln.MIR.Readback
import Coln.MIR.Syntax qualified as S
import Coln.MIR.Value qualified as V

data Memoed (s :: Case -> MLevel -> Type) (v :: Case -> MLevel -> Type) (c :: Case) (l :: MLevel) = M
  { stx :: s c l
  , val :: ~(V.Evaluation v c l)
  }

type El = Memoed S.El V.El
type Ty = Memoed S.Ty V.Ty

var :: V.Locals -> BId -> El N Set
var vs i = M (S.Var i) (levelCoerceFromMatch SSet (elemAt vs i))

fromV :: (V.HasEvaluation c, Readback (a c l) (b c l)) => CtxLen -> a c l -> Memoed b a c l
fromV n v = M (readb n v) (V.epure v)

liftEl :: (V.HasEvaluation c) => El c Set -> El c Theory
liftEl (M s v) = M (S.LiftEl s) (V.emap (V.LiftEl LSetTheory) v)

lookup :: TableName -> [El N Set] -> Ty N Set -> El N Set
lookup tn args a = M (S.Lookup tn ((.stx) <$> args) a.stx) (V.lookup tn ((.val) <$> args) a.val)

code :: SUniverse Set Theory -> Ty N Set -> El N Theory
code u (M s v) = M (S.Code u s) (V.Code u v)

primCode :: SUniverse Set Theory -> TableName -> [El N Set] -> El N Theory
primCode u tn args = M (S.PrimCode u tn ((.stx) <$> args)) (V.PrimCode u tn ((.val) <$> args))

lam :: (V.HasEvaluation c) => V.Locals -> Ty N Set -> S.Abs (S.El c Theory) -> El c Theory
lam vs dom abs = do
  let clo = evalAbs vs abs
  M (S.Lam dom.stx abs) (V.epure $ V.Lam SSetTheory dom.val clo)

cons :: (V.HasEvaluation c) => Dict (El c l) -> El c l
cons fields = M (S.Cons ((.stx) <$> fields)) (V.epure $ V.Cons ((.val) <$> fields))
