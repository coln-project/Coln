-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.SIR.Separate where

import Control.Monad (forM_)

import Control.Monad.Reader.Class

import Data.Key (mapAccumWithKeyL)

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
    V.Bundle fields -> S.Bundle $ separate n <$> fields
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

defsOf :: [(Name, S.Shape)] -> [S.Prop] -> S.Prop -> GeneratorWriter ()
defsOf cols ante = \case
  S.Atom tn _ args -> emitRule (Rule cols ante tn args)
  S.And d -> forM_ (toList d) \(x, p) -> local (\p -> p :> x) (defsOf cols ante p)
  S.Eq _ _ _ -> panic "not supported: path constructor"

addToVariants :: S.Shape -> GeneratorWriter ()
addToVariants = \case
  S.Scalar (S.RowId tn) -> emitVariantOf tn
  S.Scalar _ -> panic "not supported: constructor for primitive type"
  S.Tuple _ -> panic "not supported: constructor producing a tuple"
  S.Unstored -> pure ()

addParameterRules :: [(Name, S.Shape)] -> [S.Prop] -> S.Prop -> GeneratorWriter ()
addParameterRules ctx ante = \case
  S.Atom tn _ args ->
    emitRule $ Rule ctx ante tn args
  S.And _ -> panic "not supported: constructor producing a tuple"
  S.Eq _ _ _ -> panic "not supported: constructor producing an equality"

atomOfArgs :: GlobalSymbol -> Int -> S.Prop
atomOfArgs tn n = S.Atom tn S.Erased [S.Var (reverseId n i) | i <- fIdsUpTo n]

makePrimaryKey :: SUniverse Set Theory -> PrimaryKey -> Maybe PrimaryKey
makePrimaryKey SPropU k = Just k
makePrimaryKey SSetU _ = Nothing

separateGenerator :: V.Generator -> GeneratorWriter ()
separateGenerator gen = do
  x <- currentSymbol
  let names = toList gen.paramNames
  let types = toList gen.paramTypes
  let shapes = shapeOf <$> types
  let ctx = zip names shapes
  let n = length names
  let props = [propAt n a (V.local i) | (i, a) <- fIdsUpTo n `zip` types]
  let pkey = PrimaryKey n
  case gen.codom of
    V.GenU u -> do
      case gen.providence of
        Holy -> case u of
          SPropU -> emitTable (Table (View Materialized) ctx (Just pkey))
          SSetU -> emitTable (Table (Union BwdNil) ctx Nothing)
        Profane -> do
          emitTable (Table Ground ctx (makePrimaryKey u pkey))
          local (:> "foreignKey") $ emitLaw
            (Law Enforced ctx [atomOfArgs x n] props)
    V.GenLift a -> case hlevelOf a of
      HUnit -> pure () 
      HProp -> do
        let codProp = propAt n a V.Erased
        case gen.providence of
          Holy -> defsOf ctx props codProp
          Profane -> emitLaw (Law Monitored ctx props [codProp])
      HSet -> do
        case gen.providence of
          Holy -> do
            emitTable $ Table Cons ctx (Just pkey)
            local (:> "foreignKey") $ emitLaw
               (Law Enforced ctx [atomOfArgs x n] props)
            addToVariants (shapeOf a)
            local (:> "params") $
              addParameterRules ctx [atomOfArgs x n] (propAt n a V.Erased)
          Profane -> do
            let retName = freshNameFor names
            let retShape = shapeOf a
            let ctx' = ctx ++ [(retName, retShape)]
            let n' = n + 1
            let types' = types ++ [a]
            let props' =
                  [propAt n' a (V.local i) | (i, a) <- fIdsUpTo n' `zip` types']
            emitTable $ Table Ground ctx' (Just pkey)
            local (:> "foreignKey") $ emitLaw
              (Law Enforced ctx' [atomOfArgs x n'] props')
            local (:> "total") $ emitLaw
              (Law Monitored ctx props [atomOfArgs x n])
      _ -> panic "bad h-level of cod"
