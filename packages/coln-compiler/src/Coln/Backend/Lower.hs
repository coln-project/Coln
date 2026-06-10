module Coln.Backend.Lower where

import Coln.Common
import Coln.Core.Params
import Coln.Core.Evaluation
import Coln.Core.Syntax qualified as S
import Coln.Core.Value qualified as V
import Coln.Core.Realm qualified as C

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

data Pred
  = EltOf Term TableName [Term]
  | And [Pred]
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

data Ty = Ty
  { shape :: Shape
  , pred :: Pred
  }

separate :: CtxLen -> V.Ty N -> V.El N -> Ty
separate n = \case
  V.U _ -> panic "lowering non-set-level type: U"
  V.Decode ne -> case ne.description of
    Just (V.Record rt) -> \v -> do
      let go :: V.Locals -> [(Name, V.Locals -> V.Ty N)] -> [(Shape, Pred)]
          go _ [] = []
          go vs ((x, f):rest) = do
            let a = f vs
            let v' = V.proj v x
            let t = separate n a v'
            (t.shape, t.pred) : go (V.LSnoc vs v') rest
      let (shapes, props) = unzip $ go rt.capture (toList rt.fieldTypes)
      Ty (Tuple (withHead rt.fieldTypes shapes)) (And props)
      where
    Nothing -> panic "lowering neutral type"
  V.Function _ -> panic "lowering non-set-level type: Function"
  V.Eq et -> \_ -> Ty Unit (Equal (lower n et.lhs) (lower n et.rhs))
  V.BuiltinTy t -> \_ -> Ty (BuiltinTy t) (And [])
  V.EltOf x ts -> \v -> Ty (RowId x) (EltOf (lower n v) x (lower n <$> ts))

data Generator
  = Rel [Name] [Ty]
  | Fun [Name] [Ty] Ty

lowerAtFresh :: CtxLen -> V.Ty N -> Ty
lowerAtFresh n a = separate n a (V.local (FId n) a)

lowerTele :: [S.Ty N] -> ([Ty], V.Locals)
lowerTele = go V.LNil 0
  where
    go vs _ [] = ([], vs)
    go vs n (t:ts) = do
      let a = eval vs t
      let v = V.local (FId n) a
      let (ts', vs') = go (V.LSnoc vs v) (n + 1) ts
      (separate n a v : ts', vs')

lowerGen :: C.Generator -> Generator
lowerGen (C.Fun xs ts t) = do
  let (ts', vs) = lowerTele ts
  Fun xs ts' (lowerAtFresh (length ts) (eval vs t))
lowerGen (C.Rel xs ts) = do
  let (ts', _) = lowerTele ts
  Rel xs ts'
