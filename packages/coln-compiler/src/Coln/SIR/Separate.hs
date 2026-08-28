module Coln.SIR.Separate where

import Coln.Common
import Coln.Core.Params
import Coln.MIR.Params
import Coln.MIR.Realm qualified as V
import Coln.MIR.Value qualified as V
import Coln.SIR.Realm
import Coln.SIR.Syntax qualified as S

type CtxLen = Int

class Separate a b | a -> b where
  separate :: CtxLen -> a -> b

instance Separate V.Head (S.El Set) where
  separate n = \case
    V.Var (FId i) -> S.Var (BId (n - i - 1))
    V.Lookup tn args ret -> do
      let args' = separate (n + 1) <$> args
      let pred = S.Atom tn S.Erased (args' ++ [S.Var 0])
      S.Single $ S.Query (shapeOf ret) (S.Abs Nothing pred)

instance Separate (V.El Set) (S.El Set) where
  separate n = \case
    V.Neu ne -> do
      let go t BwdNil = t
          go t (xs :> x) = S.Proj (go t xs) x
      go (separate n ne.head) ne.spine
    V.Cons fields -> S.Cons $ separate n <$> fields
    V.Lit l -> S.Lit l
    V.Erased -> S.Erased

separateClo :: (Separate a b) => CtxLen -> V.Clo (V.El Set) a -> S.Abs b
separateClo n (V.Clo x body) = S.Abs (Just x) (separate (n + 1) (body (V.local (FId n))))
separateClo n (V.CloConst body) = S.AbsConst (separate n body)

instance Separate (V.El Theory) (S.El Theory) where
  separate n = \case
    V.LiftEl LSetTheory v -> S.LiftEl (separate n v)
    V.Code SSetU a -> S.Multi SSetU (separate n a)
    V.Code SPropU a -> S.Multi SPropU (separate n a)
    V.Lam SSetTheory dom clo -> S.Lam (separate n dom) (separateClo n clo)
    V.Cons fields -> S.Cons $ separate n <$> fields

shapeOf :: V.Ty Set -> S.Shape
shapeOf = \case
  V.EltOf SPropU _ _ -> S.Unstored
  V.EltOf SSetU x _ -> S.Scalar $ S.RowId x
  V.Record rt -> do
    let go [] _ _ = []
        go ((x, k) : rest) vs v = do
          let v' = V.proj v x
          (x, shapeOf (k vs)) : go rest (vs :> Pair SSet v') v
    let v = V.local (FId 0)
    S.Tuple $ fromList $ go (toList rt.fieldTypes) rt.capture v
  V.BuiltinTy t -> S.Scalar $ S.BuiltinTy t
  V.Eq _ _ _ -> S.Unstored

propAt :: CtxLen -> V.Ty Set -> V.El Set -> S.Prop
propAt n = \case
  V.EltOf _ x args -> \v ->
    S.Atom x (separate n v) (separate n <$> args)
  V.Record rt -> \v -> do
    let go [] _ = []
        go ((x, k) : rest) vs = do
          let v' = V.proj v x
          (x, propAt n (k vs) v') : go rest (vs :> Pair SSet v')
    S.And $ fromList $ go (toList rt.fieldTypes) rt.capture
  V.BuiltinTy _ -> \_ -> S.trueProp
  V.Eq at lhs rhs -> \_ ->
    S.Eq (shapeOf at) (separate n lhs) (separate n rhs)

instance Separate (V.Ty Set) S.Query where
  separate n a =
    S.Query (shapeOf a) (S.Abs Nothing (propAt (n + 1) a (V.local (FId n))))

separateGenerator :: TableName -> V.Generator -> (Maybe (Trie Entity), Maybe (Trie Definition), Maybe (Trie Rule))
separateGenerator tn = \case
  V.Rel u xs tys -> do
    let names = toList xs
    let argNum = length names
    let septys = uncurry separate <$> zip [0 ..] (toList tys)
    let primaryKey = case u of
          SSetU -> Nothing
          SPropU -> Just [0 .. argNum - 1]
    let table = Entity Table (zip names ((.shape) <$> septys)) primaryKey
    let atom = S.Atom tn S.Erased [S.Var (BId (argNum - i - 1)) | i <- [0 .. argNum - 1]]
    let foreignKey = Rule Enforced Consequent (zip names septys) atom S.trueProp
    (Just $ Leaf table, Nothing, Just $ Node $ fromList [("foreignKey", Leaf foreignKey)])
  V.Fun xs tys cod -> case hlevelOf cod of
    HUnit -> (Nothing, Nothing, Nothing)
    HProp -> do
      let names = toList xs
      let argNum = length names
      let septys = uncurry separate <$> zip [0 ..] (toList tys)
      let codProp = propAt argNum cod V.Erased
      let rule = Rule Monitored Antecedent (zip names septys) S.trueProp codProp
      (Nothing, Nothing, Just $ Leaf rule)
    HSet -> do
      let argNames = toList xs
      let argNum = length argNames

      let tableNameLast = case tn.path of
            (_ :> last) -> last
            BwdNil -> tn.realm
      let resultName = freshenFor xs tableNameLast
      let names = argNames ++ [resultName]

      let septys = uncurry separate <$> zip [0 ..] (toList (tys :> cod))
      let table = Entity Table (zip names ((.shape) <$> septys)) (Just [0 .. argNum - 1])

      let foreignKeyAnte = S.Atom tn S.Erased [S.Var (BId (argNum - i)) | i <- [0 .. argNum]]
      let foreignKey = Rule Enforced Consequent (zip names septys) foreignKeyAnte S.trueProp

      let totalCons = S.Atom tn S.Erased [S.Var (BId (argNum - i - 1)) | i <- [0 .. argNum - 1]]
      let total = Rule Monitored Antecedent (zip names (take argNum septys)) S.trueProp totalCons

      (Just $ Leaf table, Nothing, Just $ Node $ fromList [("foreignKey", Leaf foreignKey), ("total", Leaf total)])
    _ -> panic "bad h-level of cod"

-- separateRealm :: V.Realm -> Realm
-- separateRealm r = _
