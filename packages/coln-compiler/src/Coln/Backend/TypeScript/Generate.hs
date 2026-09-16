-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Backend.TypeScript.Generate where

import Control.Monad (forM)
import Control.Monad.State
import Data.Map.Ordered qualified as OMap
import Data.Set qualified as Set

import Coln.Backend.TypeScript.AST qualified as TS
import Coln.Common
import Coln.Core.Params
import Coln.MIR.Params

import Coln.SIR.Realm qualified as SIR
import Coln.SIR.Syntax qualified as SIR

mangle :: Name -> TS.Id
mangle = TS.Id . mangleToDoc

runtime :: TS.Id -> TS.QId
runtime x = TS.QId ["runtime"] x

setInterface :: TS.Ty -> TS.Ty
setInterface ty = TS.TyConst (runtime "Set") [ty]

mutableSetInterface :: TS.Ty -> TS.Ty
mutableSetInterface ty = TS.TyConst (runtime "MutableSet") [ty]

propInterface :: TS.Ty
propInterface = TS.TyConst (runtime "Prop") []

mutablePropInterface :: TS.Ty
mutablePropInterface = TS.TyConst (runtime "MutableProp") []

refInterface :: DefinitionType -> TS.Ty -> TS.Ty
refInterface dty ty = case dty of
  Base -> TS.TyConst (runtime "MutableRef") [ty]
  View -> TS.TyConst (runtime "Ref") [ty]

tnString :: TableName -> TS.El
tnString = TS.String . pretty . (.name)

rowId :: TableName -> TS.Ty
rowId x = TS.TyConst (runtime "RowId") [TS.Singleton $ tnString x]

class GenTy a where
  genTy :: a -> TS.Ty

instance GenTy BuiltinTy where
  genTy = \case
    BuiltinInt -> TS.TyConst "number" []
    BuiltinString -> TS.TyConst "string" []

instance GenTy SIR.ScalarType where
  genTy = \case
    SIR.RowId tn -> rowId tn
    SIR.BuiltinTy ty -> genTy ty

instance GenTy SIR.Shape where
  genTy = \case
    SIR.Tuple fields -> TS.RecordTy [(mangle x, genTy ty) | (x, ty) <- toList fields]
    SIR.Scalar s -> genTy s
    SIR.Unstored -> TS.NullTy

data DefinitionType = Base | View

instance GenTy (DefinitionType, SIR.TheoryShape) where
  genTy (dty, ty) = case ty of
    SIR.LiftTy a -> refInterface dty (genTy a)
    SIR.Function x dom cod -> TS.Fun (TS.Binding (mangle x) (genTy dom)) (genTy (dty, cod))
    SIR.Record fields -> TS.RecordTy [(mangle x, genTy (dty, ty')) | (x, ty') <- toList fields]
    SIR.BaseU pr SSetU a -> case pr of
      Profane -> mutableSetInterface (genTy a)
      Holy -> setInterface (genTy a)
    SIR.ViewU SSetU a -> setInterface (genTy a)
    SIR.BaseU pr SPropU _ -> case pr of
      Profane -> mutablePropInterface
      Holy -> propInterface
    SIR.ViewU SPropU _ -> propInterface

data FlatParams = FlatParams
  { paramVals :: Bwd TS.El
  , numParams :: Int
  }

emptyParams :: FlatParams
emptyParams = FlatParams BwdNil 0

class Reconstructable a b | a -> b, b -> a where
  atIndex :: a -> Int -> SIR.ScalarType -> b
  cons :: [(TS.Id, b)] -> b
  unstored :: b

instance Reconstructable TS.Id TS.El where
  atIndex res i = do
    let t = TS.Index (TS.Var res) i
    \case
      SIR.BuiltinTy _ -> t
      SIR.RowId tn ->
        TS.New
          (TS.Const (runtime "RowId"))
          [ TS.Object
              [ ("type", TS.String "Existing")
              , ("value", TS.Coerce t (TS.TyConst (runtime "WireRowId") []))
              ]
          , tnString tn
          ]
  cons = TS.Object
  unstored = TS.Null

instance Reconstructable () () where
  atIndex _ _ _ = ()
  cons _ = ()
  unstored = ()

allocParam :: (Reconstructable a b) => a -> TS.El -> SIR.Shape -> State FlatParams b
allocParam res v = \case
  SIR.Tuple d -> do
    fmap cons $ forM (toList d) $ \(x, sh) -> do
      let i = mangle x
      el <- allocParam res (TS.Proj v i) sh
      pure (i, el)
  SIR.Scalar st -> state \p ->
    ( atIndex res p.numParams st
    , p{paramVals = p.paramVals :> v, numParams = p.numParams + 1}
    )
  SIR.Unstored -> pure unstored

flattenParams :: [(TS.El, SIR.Shape)] -> (Int, [TS.El])
flattenParams params = do
  let fp = execState (traverse (uncurry (allocParam ())) params) (FlatParams BwdNil 0)
  (fp.numParams, toList fp.paramVals)

data Adapter = Adapter
  { flatten :: TS.El
  , reconstruct :: TS.El
  , length :: Int
  }

constructAdapter :: TSEnv -> SIR.Shape -> Adapter
constructAdapter e sh = do
  let resultVar = mangle $ freshenFor e.usedNames "result"
  let inputVar = mangle $ freshNameFor e.usedNames
  let (reconstructed, params) = runState (allocParam resultVar (TS.Var inputVar) sh) emptyParams
  let flatten =
        TS.Lam
          (TS.Binding inputVar (genTy sh))
          (TS.Block [] (Just (TS.List $ toList params.paramVals)))
  let reconstruct =
        TS.Lam
          (TS.Binding resultVar (TS.TyConst (runtime "WireTuple") []))
          (TS.Block [] (Just reconstructed))
  Adapter flatten reconstruct params.numParams

data TSEnv = TSEnv
  { tsLocals :: Bwd TS.El
  , usedNames :: Set.Set Name
  , realm :: SIR.Realm
  , store :: TS.El
  }

emptyTSEnv :: SIR.Realm -> TS.El -> TSEnv
emptyTSEnv = TSEnv BwdNil Set.empty

class GenEl a where
  genEl :: TSEnv -> a -> TS.El

createTSArgs :: TSEnv -> TableName -> [SIR.El Set] -> [(TS.El, SIR.Shape)]
createTSArgs e tn cols = do
  let tsCols = genEl e . (Value,) <$> cols
  let colShapes = snd <$> (elemAt e.realm.entities tn).columns
  zip tsCols colShapes


rowIdSet :: TSEnv -> TableName -> [SIR.El Set] -> DefinitionType -> SUniverse Set Theory -> TS.El
rowIdSet env tn cols dty u = do
  let className = case (dty, u) of
        (Base, SPropU) -> "BaseProp"
        (Base, SSetU) -> "BaseSet"
        (View, SPropU) -> "ViewProp"
        (View, SSetU) -> "InductiveViewSet"
  TS.New
    (TS.Const (runtime className))
    [ env.store
    , tnString tn
    , TS.List $ snd $ flattenParams $ createTSArgs env tn cols
    ]

projSet :: TSEnv -> TableName -> [SIR.El Set] -> SIR.Shape -> SUniverse Set Theory -> TS.El
projSet env tn cols retShape u = case u of
  SPropU -> 
    TS.New
      (TS.Const (runtime "ViewProp"))
      [ env.store
      , tnString tn
      , TS.List $ snd $ flattenParams $ createTSArgs env tn cols
      ]
  SSetU -> do
    let (n, tsParams) = flattenParams $ createTSArgs env tn cols
    let adapter = constructAdapter env retShape
    TS.New
      (TS.Const (runtime "ConjunctiveViewSet"))
      [ env.store
      , tnString tn
      , TS.List tsParams
      , TS.List [TS.Lit (LitInt i) | i <- [n .. n + adapter.length - 1]]
      , TS.Object
          [ ("flatten", adapter.flatten)
          , ("reconstruct", adapter.reconstruct)
          ]
      ]
    
baseTableRef :: TSEnv -> TableName -> [SIR.El Set] -> SIR.Shape -> TS.El
baseTableRef env tn cols retShape = do
  let (n, tsParams) = flattenParams $ createTSArgs env tn cols
  let adapter = constructAdapter env retShape
  TS.New
    (TS.Const (runtime "BaseTableRef"))
    [ env.store
    , tnString tn
    , TS.List tsParams
    , TS.List [TS.Lit (LitInt i) | i <- [n .. n + adapter.length]]
    , TS.Object
        [ ("flatten", adapter.flatten)
        , ("reconstruct", adapter.reconstruct)
        ]
    ]

viewRef :: TSEnv -> TableName -> [SIR.El Set] -> SIR.Shape -> TS.El
viewRef env tn cols retShape = do
  let (n, tsParams) = flattenParams $ createTSArgs env tn cols
  let adapter = constructAdapter env retShape
  TS.New
    (TS.Const (runtime "ViewRef"))
    [ env.store
    , tnString tn
    , TS.List tsParams
    , TS.List [TS.Lit (LitInt i) | i <- [n .. n + adapter.length]]
    , TS.Object
        [ ("flatten", adapter.flatten)
        , ("reconstruct", adapter.reconstruct)
        ]
    ]


data AccessType = Reference DefinitionType | Value

argName :: Set.Set Name -> SIR.Abs a -> Name
argName used (SIR.Abs (Just x) _) = freshenFor used x
argName used _ = freshNameFor used

instance GenEl (DefinitionType, SIR.El Theory) where
  genEl e (dty, t) = case t of
    SIR.LiftEl v -> genEl e (Reference dty, v)
    SIR.SelectRowId u tn cols -> rowIdSet e tn cols dty u
    SIR.SelectLast u tn cols retShape -> projSet e tn cols retShape u
    SIR.Lam dom abs -> do
      let x = mangle $ argName e.usedNames abs
      let tsBody = case abs of
            SIR.Abs _ body -> genEl (e{tsLocals = e.tsLocals :> TS.Var x}) (dty, body)
            SIR.AbsConst body -> genEl e (dty, body)
      TS.Lam (TS.Binding x (genTy dom.shape)) (TS.Block [] (Just tsBody))
    SIR.Cons fields -> TS.Object $ [(mangle x, genEl e (dty, t)) | (x, t) <- toList fields]

wrapConstRef :: AccessType -> TS.El -> TS.El
wrapConstRef (Reference _) t = TS.New (TS.Const (runtime "ConstRef")) [t]
wrapConstRef Value t = t

instance GenEl (AccessType, SIR.El Set) where
  genEl e (access, t) = case t of
    SIR.Var i -> wrapConstRef access $ elemAt e.tsLocals i
    SIR.Lookup tn cols retShape -> do
      let ref = baseTableRef e tn cols retShape
      case access of
        Value -> TS.MethodCall ref "value" []
        Reference _ -> ref
    SIR.Proj t x -> wrapConstRef access $ TS.Proj (genEl e (Value, t)) (mangle x)
    SIR.Cons fields -> wrapConstRef access $ TS.Object [(mangle x, genEl e (Value, t')) | (x, t') <- toList fields]
    SIR.Lit l -> wrapConstRef access $ TS.Lit l
    SIR.Erased -> wrapConstRef access $ TS.Null

genRealmConstructor :: SIR.Realm -> TS.Constructor
genRealmConstructor r = do
  let args = [TS.Binding "store" (TS.TyConst (runtime "Store") [])]
  let env = emptyTSEnv r (TS.Var "mstore")
  let auxillaryAssignments = [TS.Assign (TS.QId ["this"] (mangle x)) (genEl env (View, v)) | (x, (v, _)) <- OMap.assocs $ r.auxillaries]
  let body =
        TS.Block
          ( [ TS.Let "mstore" (TS.New (TS.Const (runtime "ManagedStore")) [TS.Var "store"])
            , TS.Assign (TS.QId ["this"] "root") (genEl env (Base, r.root))
            ]
              ++ auxillaryAssignments
          )
          Nothing
  TS.Constructor args body

genRealmClass :: Name -> SIR.Realm -> TS.Class
genRealmClass x r =
  TS.Class
    (mangle x)
    Nothing
    Nothing
    (TS.Binding "root" (genTy (Base, r.rootType)) : [TS.Binding (mangle x) (genTy (View, ty)) | (x, (_, ty)) <- OMap.assocs r.auxillaries])
    (genRealmConstructor r)

genRealmModule :: Name -> SIR.Realm -> TS.Module
genRealmModule x r = do
  TS.Module
    [TS.ImportQualified "runtime" "@coln-project/interface"]
    [TS.Exported $ TS.DClass $ genRealmClass x r]
