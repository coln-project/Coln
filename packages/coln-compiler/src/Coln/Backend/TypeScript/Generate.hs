-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Backend.TypeScript.Generate where

import Control.Monad.State

-- import Data.Aeson qualified as AE
-- import Data.Foldable (foldlM)

-- import Data.Foldable qualified as F
-- import Data.Map.Ordered qualified as OMap
import Data.Set qualified as Set
-- import Data.String (IsString (..))
-- import Data.Text.Lazy qualified as TL
-- import Data.Text.Lazy.IO qualified as TLIO
-- import Prettyprinter
-- import Prettyprinter.Render.Text
-- import System.FilePath

import Coln.Backend.TypeScript.AST qualified as TS
-- import Coln.Backend.TypeScript.Assemble (asm)
import Coln.Common
import Coln.Core.Params
import Coln.MIR.Params

import Coln.FLIR.Flatten qualified as FLIR
import Coln.FLIR.Value qualified as FLIR
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

refInterface :: TS.Ty -> TS.Ty
refInterface ty = TS.TyConst (runtime "Ref") [ty]

mutableRefInterface :: TS.Ty -> TS.Ty
mutableRefInterface ty = TS.TyConst (runtime "MutableRef") [ty]

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
    SIR.Tuple fields -> TS.RecordTy (genTy <$> fields)
    SIR.Scalar s -> genTy s
    SIR.Unstored -> TS.NullTy

instance GenTy SIR.TheoryShape where
  genTy = \case
    SIR.LiftTy a -> refInterface (genTy a)
    SIR.Function x dom cod -> TS.Fun (TS.Binding (mangle x) (genTy dom)) (genTy cod)
    SIR.Record fields -> TS.RecordTy (genTy <$> fields)
    SIR.U SSetU a -> setInterface (genTy a)
    SIR.U SPropU _ -> propInterface

data FlatParams = FlatParams
  { paramVals :: Bwd TS.El
  , numParams :: Int
  }

allocParam :: TS.El -> SIR.Shape -> State FlatParams FLIR.Els
allocParam v = \case
  SIR.Tuple d -> do
    FLIR.Cons <$> mapWithKeyM (\x sh -> allocParam (TS.Proj v (mangle x)) sh) d
  SIR.Scalar _ -> state \p ->
    ( FLIR.Scalar (FLIR.Param (FId p.numParams))
    , p{paramVals = p.paramVals :> v, numParams = p.numParams + 1}
    )
  SIR.Unstored -> pure FLIR.Erased

-- allocParams :: [(TS.El, SIR.Shape)] -> State FlatParams [TS.El]

data TSEnv = TSEnv
  { tsLocals :: Bwd TS.El
  , usedNames :: Set.Set Name
  , realm :: SIR.Realm
  , store :: TS.El
  }

emptyTSEnv :: SIR.Realm -> TS.El -> TSEnv
emptyTSEnv = TSEnv BwdNil Set.empty

reconstructEl :: FlatParams -> FLIR.El -> TS.El
reconstructEl e = \case
  FLIR.LocalVar (FId i) -> TS.Index (TS.Var "result") i
  FLIR.Lit l -> TS.Lit l
  FLIR.Param (FId i) -> elemAt e.paramVals (BId (e.numParams - i - 1))

reconstructEls :: FlatParams -> FLIR.Els -> TS.El
reconstructEls e = \case
  FLIR.Scalar v -> reconstructEl e v
  FLIR.Cons d -> TS.Object [(mangle x, reconstructEls e t) | (x, t) <- toList d]
  FLIR.Erased -> TS.Null

class GenEl a where
  genEl :: TSEnv -> a -> TS.El

flattenParams :: [(TS.El, SIR.Shape)] -> [TS.El]
flattenParams params = do
  let fp = execState (traverse (uncurry allocParam) params) (FlatParams BwdNil 0)
  toList fp.paramVals

baseTableSet :: TSEnv -> TableName -> [(TS.El, SIR.Shape)] -> TS.El
baseTableSet env tn params =
  TS.New
    (TS.Const (runtime "BaseTableSet"))
    [ env.store
    , tnString tn
    , TS.List $ flattenParams params
    ]

viewTableSet :: TSEnv -> TableName -> [(TS.El, SIR.Shape)] -> SIR.Shape -> TS.El
viewTableSet env tn params retShape = undefined

data El = Reference (SIR.El Set) | Value (SIR.El Set)

argName :: Set.Set Name -> SIR.Abs a -> Name
argName used (SIR.Abs (Just x) _) = freshenFor used x
argName used _ = freshNameFor used

instance GenEl (SIR.El Theory) where
  genEl e = \case
    SIR.LiftEl v -> genEl e (Reference v)
    SIR.SelectRowId u tn cols -> do
      let tsCols = genEl e . Value <$> cols
      let colShapes = snd <$> (elemAt e.realm.entities tn).columns
      case u of
        SPropU -> panic "todo"
        SSetU -> baseTableSet e tn $ zip tsCols colShapes
    SIR.SelectLast u tn cols retShape -> do
      let tsCols = genEl e . Value <$> cols
      let colShapes = snd <$> (elemAt e.realm.entities tn).columns
      case u of
        SPropU -> panic "todo"
        SSetU -> viewTableSet e tn (zip tsCols colShapes) retShape
    SIR.Lam dom abs -> do
      let x = mangle $ argName e.usedNames abs
      let tsBody = case abs of
            SIR.Abs _ body -> genEl (e { tsLocals = e.tsLocals :> TS.Var x }) body
            SIR.AbsConst body -> genEl e body
      TS.Lam (TS.Binding x (genTy dom.shape)) (TS.Block [] (Just tsBody))
    SIR.Cons fields -> TS.Object $ [(mangle x, genEl e t) | (x, t) <- toList fields]

instance GenEl El where
  genEl e = \case
    _ -> undefined

-- genQuery :: Access -> TSEnv -> SIR.Query -> TS.El
-- genQuery _access e q = do
--   let ((v, mainProps), vars, auxProps) = FLIR.runFlatM $ do
--         v <- FLIR.freshAt (BwdNil :> "result") q.shape
--         mainprops <- FLIR.app e.flirLocals q.pred v
--         pure (v, mainprops)
--   let query = FLIR.Query vars (toList (mainProps <> auxProps))
--   let flir = TS.String (undefined query)
--   -- TODO: should make sure "result" is fresh
--   let reconstruct =
--         TS.Lam
--           (TS.Binding "result" (TS.ListTy (TS.runtime Value)))
--           (TS.Block [] (Just (reconstructEls e.flatParams v)))
--   TS.New (TS.Const (TS.runtime Query)) [flir, reconstruct]

-- varName :: SIR.Abs a -> Set.Set Name -> Name
-- varName (SIR.Abs (Just x) _) xs = case Set.member x xs of
--   True -> freshNameFor xs
--   False -> x
-- varName _ xs = freshNameFor xs

-- genAbs :: Access -> TSEnv -> SIR.Abs (SIR.El l) -> (Name, TS.El)
-- genAbs access e (SIR.Abs mx body) = do
--   let x = freshNameWithPref e.usedNames mx
--   let e' =
--         e
--           { tsLocals = e.tsLocals :> TS.Var (mangle x)
--           , usedNames = Set.insert x e.usedNames
--           }
--   (x, genEl access e' body)
-- genAbs access e (SIR.AbsConst body) = do
--   let x = freshNameFor e.usedNames
--   let e' = e{usedNames = Set.insert x e.usedNames}
--   (x, genEl access e' body)

-- genEl :: Access -> TSEnv -> SIR.El l -> TS.El
-- genEl access e = \case
--   SIR.LiftEl t -> genEl access e t
--   SIR.Var i -> elemAt e.tsLocals i
--   -- SIR.Single q -> TS.MethodCall (genQuery access e q) "single" []
--   SIR.Proj t x -> TS.Proj (genEl access e t) (mangle x)
--   -- SIR.Multi _ q -> TS.MethodCall (genQuery access e q) "multi" []
--   SIR.Lam _dom abs -> do
--     let (x, body) = genAbs access e abs
--     TS.Lam
--       (TS.Binding (mangle x) (TS.runtime Value))
--       (TS.Block [] (Just body))
--   SIR.Cons fields ->
--     TS.Object [(mangle x, genEl access e t) | (x, t) <- toList fields]
--   SIR.Lit l -> TS.Lit l
--   SIR.Erased -> TS.Null

-- genRealmConstructor :: Access -> SIR.Realm -> TS.Constructor
-- genRealmConstructor access r = do
--   let args = case access of
--         View ->
--           [ TS.Binding "store" (TS.runtime StoreHandle)
--           ]
--         Transaction ->
--           [ TS.Binding "store" (TS.runtime StoreHandle)
--           , TS.Binding "transaction" (TS.runtime TransactionHandle)
--           ]
--   let superCall = case extends access of
--         Just _ -> [TS.Expr (TS.Call (TS.Var "super") [TS.Var "store"])]
--         Nothing -> []
--   let body =
--         TS.Block
--           (superCall ++ [TS.Assign (TS.QId ["this"] "root") (genEl access emptyTSEnv r.root)])
--           Nothing
--   TS.Constructor args body

-- genRealmClass :: Access -> SIR.Realm -> TS.Class
-- genRealmClass access r =
--   TS.Class
--     (fromShow access)
--     Nothing
--     (fromShow <$> extends access)
--     [TS.Binding "root" (genTy access 0 r.rootType)]
--     (genRealmConstructor access r)

-- genRealmModule :: [TS.Import] -> SIR.Realm -> TS.Module
-- genRealmModule imports r = do
--   let classes = for accessLevels $ \access -> TS.DClass $ genRealmClass access r
--   TS.Module imports (TS.Exported <$> classes)

-- render :: DDoc -> TL.Text
-- render = renderLazy . layoutPretty defaultLayoutOptions

-- writeModule :: FilePath -> Name -> TS.Module -> IO ()
-- writeModule outdir x mod = do
--   let fn = outdir </> TS.idToString (mangle x) <> ".ts"
--   let content = render $ asm mod
--   TLIO.writeFile fn content

-- runtimeImport :: TS.Import
-- runtimeImport = TS.ImportQualified "runtime" "@coln-project/runtime"

-- forAccM :: (Monad m) => [b] -> a -> (a -> b -> m a) -> m a
-- forAccM bs init f = foldlM f init bs

-- generate :: Globals -> FilePath -> IO ()
-- generate ge outdir = do
--   typeImports <- forAccM (OMap.assocs ge.definitions) BwdNil $ \imports (x, e) -> do
--     let ev = e.body.val :: V.Evaluation V.El D
--     case genEntryModule (runtimeImport : toList imports) e.ty ev of
--       Just mod -> do
--         writeModule outdir x mod
--         pure (imports :> TS.ImportQualified (mangle x) ("./" <> mangleToDoc x <> ".ts"))
--       Nothing -> pure imports
--   let imports = runtimeImport : toList typeImports
--   forM_ (OMap.assocs ge.realms) $ \(x, r) -> do
--     let flat = lowerRealm x r
--     flip AE.encodeFile flat $ outdir </> mangleToString x <> ".json"
--     let schemaImport = TS.ImportSpecificExported "schema" $ "./" <> mangleToDoc x <> ".json"
--     let mod = genRealmModule (schemaImport : imports) r
--     writeModule outdir x mod
