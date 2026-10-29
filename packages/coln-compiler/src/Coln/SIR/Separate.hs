-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.SIR.Separate where

import Control.Arrow (second)
import Control.Monad.Reader.Class
import Control.Monad.State.Class
import Control.Monad.RWS.Strict
import Data.Key (mapAccumWithKeyL)
import Data.Maybe (mapMaybe, maybeToList)
import Data.Map.Ordered qualified as OMap

import Coln.Common
import Coln.Core.Params
import Coln.MIR.Layout (argName)
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
    V.Var i -> S.Var (reverseId n i)
    V.Lookup tn args ret -> case shapeOf ret of
      S.Unstored -> S.Erased
      sh -> S.Lookup tn (separate n <$> args) sh

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

theoryShapeOf :: CtxShape -> V.Ty N Theory -> V.El N Theory -> S.TheoryShape
theoryShapeOf cs a v = case a of
  V.LiftTy LSetTheory a' -> S.LiftTy (shapeOf a')
  V.Function ft -> case ft.variant.mlevel of
    SSetTheory -> do
      let arg = V.local (FId cs.len)
      let dom = shapeOf ft.dom
      let x = argName cs.names ft.cod
      let cs' = CtxShape (cs.len + 1) (cs.names :> x)
      let cod = theoryShapeOf cs' (V.appClo ft.cod arg) (V.app SSetTheory v arg)
      S.Function x dom cod
  V.Record rt -> do
    let doField x l fieldTy = do
          let fieldVal = V.proj v x
          (l :> Pair STheory fieldVal, theoryShapeOf cs (fieldTy l) fieldVal)
    let fields = snd $ mapAccumWithKeyL doField rt.capture rt.fieldTypes
    S.Record fields
  V.U (inferSetCodes -> u) -> case v of
    V.PrimCode pr _ tn _ -> S.BaseU pr u (S.Scalar (S.RowId tn))
    V.Code SSetU a -> S.ViewU u $ shapeOf a
    V.Code SPropU a -> S.ViewU u $ shapeOf a
    _ -> panic "expected a primcode or a code"

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

-- We don't need the whole `Maybe Trie` dance!

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

-- We should refactor this to essentially be in a monad where we can emit
-- entities, definitions, etc?

newtype GeneratorWriter a = GeneratorWriter
  { runGeneratorWriter :: RWS Path () Generators a
  }
  deriving (Functor, Applicative, Monad, MonadReader Path, MonadState Generators)

emitEntity :: Entity -> GeneratorWriter ()
emitEntity e = do
  p <- ask
  modify (\gs -> gs { entities = gs.entities OMap.>| (tableName p, e) })

emitDefinition :: Name -> Definition -> GeneratorWriter ()
emitDefinition x d = do
  p <- ask
  modify (\gs -> gs { definitions = gs.definitions OMap.>| (tableName $ p :> x, d) })

emitRule :: Maybe Name -> Rule -> GeneratorWriter ()
emitRule mx l = do
  p <- ask
  let p' = case mx of
        Just x -> p :> x
        Nothing -> p
  modify (\gs -> gs { rules = gs.rules OMap.>| (tableName p', l) })

emitUnion :: GeneratorWriter ()
emitUnion = do
  p <- ask
  modify (\gs -> gs { unions = gs.unions OMap.>| (tableName p, Union BwdNil) })

emitCaseFor :: TableName -> GeneratorWriter ()
emitCaseFor tn = do
  p <- ask
  modify $ \gs -> do
   let u = elemAt gs.unions tn
   gs { unions = gs.unions OMap.|> (tn, Union (u.tables :> tableName p)) }

separateGenerator :: TableName -> V.Generator -> GeneratorWriter ()
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
            Holy -> View Materialized
            Profane -> Table
      emitEntity (Entity entityVariant (second (.shape) <$> cols) primaryKey)
      case gen.providence of
        Holy -> pure ()
        Profane -> do
          let atom = S.Atom tn S.Erased [S.Var (BId $ argNum - i - 1) | i <- [0 .. argNum - 1]]
          emitRule
            (Just "foreignKey")
            (Rule Enforced Consequent (zip names septys) atom S.trueProp)
    V.GenLift a -> case hlevelOf a of
      HUnit -> pure () 
      HProp -> do
        let codProp = propAt argNum a V.Erased
        case gen.providence of
          Holy -> defsOf cols codProp
          Profane ->
            emitRule Nothing (Rule Monitored Antecedent cols S.trueProp codProp)
      HSet -> do
        let resultName = freshNameFor names
        let resultQ = separate argNum a

        case gen.providence of
          Holy -> do
            emitEntity $ Entity Hashcons (second (.shape) <$> cols) (Just [0 .. argNum - 1])

            let atom = S.Atom tn S.Erased [S.Var (BId (argNum - i - 1)) | i <- [0 .. argNum - 1]]
            emitRule
              (Just "foreignKey")
              (Rule Enforced Consequent cols S.trueProp atom)

            let domProp = S.Atom tn S.Erased 
            let domQ = S.Query S.Unstored (S.Abs Nothing domProp)
            let codProp = propAt (argNum + 1) a V.Erased
            defsOf (cols ++ [(resultName, domQ)]) codProp
          Profane -> do
            let allCols = cols ++ [(resultName, resultQ)]
            emitEntity $ Entity Table (second (.shape) <$> allCols) (Just [0 .. argNum - 1])

            let foreignKeyAnte = S.Atom tn S.Erased [S.Var (BId (argNum - i)) | i <- [0 .. argNum]]
            emitRule
              (Just "foreignKey")
              (Rule Enforced Consequent allCols foreignKeyAnte S.trueProp)

            let totalCons = S.Atom tn S.Erased [S.Var (BId (argNum - i - 1)) | i <- [0 .. argNum - 1]]
            emitRule
              (Just "total")
              (Rule Monitored Antecedent cols S.trueProp totalCons)
      _ -> panic "bad h-level of cod"
