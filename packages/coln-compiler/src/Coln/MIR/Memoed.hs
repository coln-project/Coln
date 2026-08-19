module Coln.MIR.Memoed where

import Coln.Common
import Coln.Core.Params
import Coln.MIR.Params
import Coln.MIR.Syntax qualified as S
import Coln.MIR.Value qualified as V
import Coln.MIR.Readback
import Coln.MIR.Evaluation

data Memoed (s :: MLevel -> Type) (v :: MLevel -> Type) (l :: MLevel) = M
  { stx :: s l
  , val :: ~(v l)
  }

type El = Memoed S.El V.El
type Ty = Memoed S.Ty V.Ty

var :: V.Locals -> BId -> El Set
var vs i = M (S.Var i) (levelCoerceFromMatch SSet (elemAt vs i))

fromV :: (Readback (a l) (b l)) => CtxLen -> a l -> Memoed b a l
fromV n v = M (readb n v) v

liftEl :: El Set -> El Theory
liftEl (M s v) = M (S.LiftEl s) (V.LiftEl LSetTheory v)

lookup :: TableName -> [El Set] -> Ty Set -> El Set
lookup tn args a = M (S.Lookup tn ((.stx) <$> args) a.stx) (V.lookup tn ((.val) <$> args) a.val)

code :: SUniverse Set Theory -> Ty Set -> El Theory
code u (M s v) = M (S.Code u s) (V.Code u v)

eltOf :: TableName -> [El Set] -> Ty Set
eltOf tn args = M (S.EltOf tn ((.stx) <$> args)) (V.EltOf tn ((.val) <$> args))

lam :: V.Locals -> S.Abs (S.El Theory) -> El Theory
lam vs abs = do
  let clo = evalAbs vs abs
  M (S.Lam abs) (V.Lam SSetTheory clo)

cons :: Dict (El l) -> El l
cons fields = M (S.Cons ((.stx) <$> fields)) (V.Cons ((.val) <$> fields))
