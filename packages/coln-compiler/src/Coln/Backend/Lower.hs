module Coln.Backend.Lower where

import Coln.Common
import Coln.Core.Params
import Coln.Core.Syntax qualified as S
import Coln.Core.Value qualified as V

data Shape
  = RowId TableName
  | BuiltinTy BuiltinTy
  | Tuple (Dict Shape)
  | Unit

data Term
  = Var BId
  | Lookup TableName [Term]
  | Cons (Dict Term)
  | Proj Term Name
  | Lit Literal

data Proposition
  = EltOf Term TableName [Term]
  | And [Proposition]
  | Equal Term Term

type CtxLen = Int

class Lower a b | a -> b where
  lower :: CtxLen -> a -> b

instance Lower V.Head Term where
  lower n (V.LocalVar (FId i)) = Var (BId (n - i - 1))
  lower _ (V.GlobalVar _ _) = panic "not fully evaluated"

instance Lower V.Spine (Term -> Term) where
  lower n = \case
    V.Id -> \t -> t
    V.App _ _ -> panic "not fully laid out"
    V.Proj sp x -> \t -> Proj (lower n sp t) x

instance Lower V.Neutral Term where
  lower n ne = lower n ne.spine $ lower n ne.head

instance Lower (V.El N) Term where
  lower :: CtxLen -> V.El N -> Term
  lower n = \case
    V.Neu ne -> lower n ne
    V.Code _ -> panic "non set-level term"
    V.Lam _ _ -> panic "non set-level term"
    V.Cons ds -> Cons (lower n <$> ds)
    V.Lit l -> Lit l
    V.Lookup x ts -> Lookup x (lower n <$> ts)

separate :: CtxLen -> V.Ty N -> V.El N -> (Shape, Proposition)
separate n = \case
  V.U _ -> panic "lowering non-set-level type: U"
  V.Decode ne -> case ne.description of
    Just (V.Record rt) -> \v -> do
      let go :: V.Locals -> [(Name, V.Locals -> V.Ty N)] -> [(Shape, Proposition)]
          go _ [] = []
          go vs ((x, f):rest) = do
            let a = f vs
            let v' = V.proj v x
            let (s, p) = separate n a v'
            (s,p) : go (V.LSnoc vs v') rest
      let (shapes, props) = unzip $ go rt.capture (toList rt.fieldTypes)
      (Tuple (withHead rt.fieldTypes shapes), And props)
      where
    Nothing -> panic "lowering neutral type"
  V.Function _ -> panic "lowering non-set-level type: Function"
  V.Eq et -> \_ -> (Unit, Equal (lower n et.lhs) (lower n et.rhs))
  V.BuiltinTy t -> \_ -> (BuiltinTy t, And [])
  V.EltOf x ts -> \v -> (RowId x, EltOf (lower n v) x (lower n <$> ts))
