module Coln.SIR.Separate where

import Coln.Common
import Coln.Core.Params
import Coln.MIR.Params
import Coln.MIR.Realm qualified as V
import Coln.MIR.Value qualified as V
import Coln.SIR.Realm
import Coln.SIR.Syntax qualified as S

import Control.Arrow (second)
import Data.Maybe (mapMaybe, maybeToList)

type CtxLen = Int

class Separate a b | a -> b where
  separate :: CtxLen -> a -> b

instance Separate V.Head (S.El Set) where
  separate n = \case
    V.Var (FId i) -> S.Var (BId (n - i - 1))
    V.Lookup tn args ret -> S.Lookup tn (separate n <$> args) (shapeOf ret)

instance Separate (V.El N Set) (S.El Set) where
  separate n = \case
    V.Neu ne -> do
      let go t BwdNil = t
          go t (xs :> x) = S.Proj (go t xs) x
      go (separate n ne.head) ne.spine
    V.Cons fields -> S.Cons $ separate n <$> fields
    V.Lit l -> S.Lit l
    V.Erased -> S.Erased

separateClo :: (Separate a b) => CtxLen -> V.Clo (V.El N Set) a -> S.Abs b
separateClo n (V.Clo x body) = S.Abs (Just x) (separate (n + 1) (body (V.local (FId n))))
separateClo n (V.CloConst body) = S.AbsConst (separate n body)

-- instance Separate (V.El N Theory) (S.El Theory) where
--   separate n = \case
--     V.LiftEl LSetTheory v -> S.LiftEl (separate n v)
--     V.Code SSetU a -> S.Multi SSetU (separate n a)
--     V.Code SPropU a -> S.Multi SPropU (separate n a)
--     V.Lam SSetTheory dom clo -> S.Lam (separate n dom) (separateClo n clo)
--     V.Cons fields -> S.Cons $ separate n <$> fields

shapeOf :: V.Ty N Set -> S.Shape
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

propAt :: CtxLen -> V.Ty N Set -> V.El N Set -> S.Prop
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

instance Separate (V.Ty N Set) S.Query where
  separate n a =
    S.Query (shapeOf a) (S.Abs Nothing (propAt (n + 1) a (V.local (FId n))))

defsOf :: [(Name, S.Query)] -> S.Prop -> Maybe (Trie Definition)
defsOf cols = \case
  S.Atom tn _ args -> Just $ Leaf $ Definition cols tn args
  S.And d -> do
    -- this should not be possible, but if it were, it should do this
    let pairs = (mapMaybe . traverse) (defsOf cols) $ toList d
    case pairs of
      [] -> Nothing
      _ -> Just $ Node $ fromList pairs
  S.Eq _ _ _ -> panic "not supported: path constructor"

separateGenerator :: TableName -> V.Generator -> (Maybe (Trie Entity), Maybe (Trie Definition), Maybe (Trie Rule))
separateGenerator tn gen = do
  let names = toList gen.paramNames
  let argNum = length names
  let septys = uncurry separate <$> zip [0 ..] (toList gen.paramTypes)
  let cols = zip names septys
  case gen.codom of
    V.GenU u -> do
      let primaryKey = case u of
            SSetU -> Nothing
            SPropU -> Just [0 .. argNum - 1]
      let entityVariant = case gen.providence of
            V.Holy -> View Memoized
            V.Profane -> Table
      let entity = Entity entityVariant (second (.shape) <$> cols) primaryKey
      let atom = S.Atom tn S.Erased [S.Var (BId $ argNum - i - 1) | i <- [0 .. argNum - 1]]
      let foreignKey = Rule Enforced Consequent (zip names septys) atom S.trueProp
      let rules = case gen.providence of
            V.Holy -> Nothing
            V.Profane -> Just $ Node $ fromList [("foreignKey", Leaf foreignKey)]
      (Just $ Leaf entity, Nothing, rules)
    V.GenLift a -> case hlevelOf a of
      HUnit -> (Nothing, Nothing, Nothing)
      HProp -> do
        let codProp = propAt argNum a V.Erased
        case gen.providence of
          V.Holy -> do
            (Nothing, defsOf cols codProp, Nothing)
          V.Profane -> do
            let rule = Rule Monitored Antecedent cols S.trueProp codProp
            (Nothing, Nothing, Just $ Leaf rule)
      HSet -> do
        let tableNameLast = case tn.path of
              (_ :> last) -> last
              BwdNil -> tn.realm
        let resultName = freshenFor names tableNameLast
        let resultQ = separate argNum a

        case gen.providence of
          V.Holy -> do
            let view = Entity (View Memoized) (second (.shape) <$> cols) (Just [0 .. argNum - 1])

            let collect = Definition cols tn [S.Var (BId (argNum - i - 1)) | i <- [0 .. argNum - 1]]

            let domProp = S.Atom tn S.Erased [S.Var (BId (argNum - i - 1)) | i <- [0 .. argNum - 1]]
            let domQ = S.Query S.Unstored (S.Abs Nothing domProp)
            let codProp = propAt (argNum + 1) a V.Erased
            let construct = defsOf (cols ++ [(resultName, domQ)]) codProp
            (Just $ Leaf view, Just $ Node $ fromList $ ("collect", Leaf collect) : (maybeToList . sequence) ("construct", construct), Nothing)
          V.Profane -> do
            let allCols = cols ++ [(resultName, resultQ)]
            let table = Entity Table (second (.shape) <$> allCols) (Just [0 .. argNum - 1])

            let foreignKeyAnte = S.Atom tn S.Erased [S.Var (BId (argNum - i)) | i <- [0 .. argNum]]
            let foreignKey = Rule Enforced Consequent allCols foreignKeyAnte S.trueProp

            let totalCons = S.Atom tn S.Erased [S.Var (BId (argNum - i - 1)) | i <- [0 .. argNum - 1]]
            let total = Rule Monitored Antecedent cols S.trueProp totalCons

            (Just $ Leaf table, Nothing, Just $ Node $ fromList [("foreignKey", Leaf foreignKey), ("total", Leaf total)])
      _ -> panic "bad h-level of cod"

-- separateRealm :: V.Realm -> Realm
-- separateRealm r = _
