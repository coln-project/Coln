-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.SIR.Separate where

import Control.Monad (forM_)
import Control.Arrow (second)
import Control.Monad.Reader.Class
import Control.Monad.State.Class
import Control.Monad.RWS.Strict
import Data.Key (mapAccumWithKeyL)

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


-- We should refactor this to essentially be in a monad where we can emit
-- entities, definitions, etc?

newtype GeneratorWriter a = GeneratorWriter
  { runGeneratorWriter :: RWS Path () Generators a
  }
  deriving (Functor, Applicative, Monad, MonadReader Path, MonadState Generators)

emitEntity :: Entity -> GeneratorWriter ()
emitEntity e = do
  p <- ask
  modify (\gs -> gs { entities = gs.entities OMap.>| (manglePath p, e) })

emitDefinition :: Definition -> GeneratorWriter ()
emitDefinition d = do
  p <- ask
  modify (\gs -> gs { definitions = gs.definitions OMap.>| (manglePath p, d) })

emitRule :: Maybe Name -> Rule -> GeneratorWriter ()
emitRule mx l = do
  p <- ask
  let p' = case mx of
        Just x -> p :> x
        Nothing -> p
  modify (\gs -> gs { rules = gs.rules OMap.>| (manglePath p', l) })

emitUnion :: GeneratorWriter ()
emitUnion = do
  p <- ask
  modify (\gs -> gs { unions = gs.unions OMap.>| (manglePath p, Union BwdNil) })

emitCaseFor :: GlobalSymbol -> GeneratorWriter ()
emitCaseFor tn = do
  p <- ask
  modify $ \gs -> do
   let u = elemAt gs.unions tn
   gs { unions = gs.unions OMap.|> (tn, Union (u.variants :> manglePath p)) }

defsOf :: [(Name, S.Query)] -> S.Prop -> GeneratorWriter ()
defsOf cols = \case
  S.Atom tn _ args -> emitDefinition (Definition cols tn args)
  S.And d -> forM_ (toList d) \(x, p) -> local (\p -> p :> x) (defsOf cols p)
  S.Eq _ _ _ -> panic "not supported: path constructor"

atomOfArgs :: GlobalSymbol -> Int -> S.Prop
atomOfArgs tn n = S.Atom tn S.Erased [S.Var (reverseId n i) | i <- fIdsUpTo n]

separateGenerator :: GlobalSymbol -> V.Generator -> GeneratorWriter ()
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
            Holy -> case u of
              SPropU -> View Materialized
              SSetU -> Cons
            Profane -> Ground
      emitEntity (Entity entityVariant (second (.shape) <$> cols) primaryKey)
      case entityVariant of
        (Cons; Ground) -> do
          emitRule
            (Just "foreignKey")
            (Rule Enforced Consequent (zip names septys) (atomOfArgs tn argNum) S.trueProp)
        _ -> pure ()
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
            emitEntity $ Entity Cons (second (.shape) <$> cols) (Just [0 .. argNum - 1])
            let atom = atomOfArgs tn argNum
            emitRule
              (Just "foreignKey")
              (Rule Enforced Consequent cols S.trueProp atom)
            let domProp = S.Atom tn S.Erased []
            let domQ = S.Query S.Unstored (S.Abs Nothing domProp)
            let codProp = propAt (argNum + 1) a V.Erased
            defsOf (cols ++ [(resultName, domQ)]) codProp
          Profane -> do
            let allCols = cols ++ [(resultName, resultQ)]
            emitEntity $ Entity Ground (second (.shape) <$> allCols) (Just [0 .. argNum - 1])
            let foreignKeyAnte = atomOfArgs tn (argNum + 1)
            emitRule
              (Just "foreignKey")
              (Rule Enforced Consequent allCols foreignKeyAnte S.trueProp)
            let totalCons = atomOfArgs tn argNum
            emitRule
              (Just "total")
              (Rule Monitored Antecedent cols S.trueProp totalCons)
      _ -> panic "bad h-level of cod"
