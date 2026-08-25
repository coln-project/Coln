-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Backend.TypeScript.Generate where


import Control.Monad.State
-- import Data.Aeson qualified as AE
import Data.Foldable (foldlM)
-- import Data.Foldable qualified as F
-- import Data.Map.Ordered qualified as OMap
import Data.Set qualified as Set
import Data.String (IsString (..))
import Data.Text.Lazy qualified as TL
import Data.Text.Lazy.IO qualified as TLIO
import Prettyprinter
import Prettyprinter.Render.Text
import System.FilePath

import Coln.Backend.TypeScript.AST qualified as TS
import Coln.Backend.TypeScript.Assemble (asm)
import Coln.Backend.TypeScript.Params
import Coln.Common

import Coln.Core.Params
import Coln.Core.Readback
import Coln.Core.Value qualified as V
import Coln.Core.Syntax qualified as S
import Coln.SIR.Syntax qualified as SIR
import Coln.SIR.Realm qualified as SIR
import Coln.FLIR.Flatten qualified as FLIR
import Coln.FLIR.Value qualified as FLIR

mangle :: Name -> TS.Id
mangle = TS.Id . mangleToDoc

tyFromHead :: Access -> V.Head -> TS.Ty
tyFromHead access (V.GlobalVar x _) =
  TS.TyConst (TS.QId [mangle x] (fromString (show access)))
tyFromHead access (V.LocalVar _) = TS.runtime $ ColnRef access

genTy :: Access -> CtxLen -> V.Ty N -> TS.Ty
genTy access n = \case
  V.U (SetU; PropU) -> TS.runtime (ColnSet access)
  V.Function ft -> do
    let v = V.local (FId n) ft.dom
    TS.Fun (TS.Binding (TS.Id "x") (TS.runtime Value)) (genTy access (n + 1) (V.appClo ft.cod v))
  V.Decode n -> tyFromHead access n.head
  V.BuiltinTy _ -> TS.runtime $ ColnRef access
  _ -> error "not yet supported"

genInterface :: Access -> CtxLen -> V.Ty D -> TS.Interface
genInterface access n = \case
  V.Record rt -> do
    let name = fromString $ show access
    let extendsName = fromString . show <$> extends access
    TS.Interface name extendsName (go n rt.capture (toList rt.fieldTypes))
   where
    go _ _ [] = []
    go n' vs ((x, f) : rest) = do
      let a = f vs
      let v = V.local (FId n') a
      let bnd = TS.Binding (mangle x) (genTy access n' a)
      bnd : go (n' + 1) (V.LSnoc vs v) rest

class TrackGlobals a where
  trackGlobals :: a -> State (Set.Set Name) ()

instance TrackGlobals (f c) => TrackGlobals (S.Abs f c) where
  trackGlobals (S.Abs _ body) = trackGlobals body
  trackGlobals (S.AbsConst body) = trackGlobals body

instance TrackGlobals a => TrackGlobals (Name, a) where
  trackGlobals (_, t) = trackGlobals t

instance TrackGlobals (S.El c) where
  trackGlobals = \case
    S.LocalVar _ -> pure ()
    S.GlobalVar x _ -> modify (Set.insert x)
    S.Code _ a -> trackGlobals a
    S.Lam _ dom body -> do
      trackGlobals dom
      trackGlobals body
    S.App _ t0 t1 -> do
      trackGlobals t0
      trackGlobals t1
    S.Cons _ ts -> mapM_ trackGlobals (toList ts)
    S.Proj _ t _ -> trackGlobals t
    S.Init _ -> pure ()
    S.Lit _ -> pure ()
    S.Is t -> trackGlobals t

instance TrackGlobals (S.Ty c) where
  trackGlobals = \case
    S.U _ -> pure ()
    S.Decode _ t -> trackGlobals t
    S.Function ft -> do
      trackGlobals ft.dom
      trackGlobals ft.cod
    S.Record rt -> mapM_ trackGlobals (toList rt.fieldTypes)
    S.Eq et -> do
      trackGlobals et.lhs
      trackGlobals et.rhs
    S.BuiltinTy _ -> pure ()
    S.IsTy a -> trackGlobals a

genTypeDef :: Access -> CtxLen -> V.Ty N -> TS.TypeDef
genTypeDef access n a = TS.TypeDef (fromShow access) (genTy access n a)

genEntryModule :: [TS.Import] -> V.Ty N -> V.Evaluation V.El D -> Maybe TS.Module
genEntryModule imports a ev = go 0 a ev
 where
  go :: CtxLen -> V.Ty N -> V.Evaluation V.El D -> Maybe TS.Module
  go n (V.U TheoryU) ev' = do
    let definitions = for accessLevels $ \access ->
          case V.ebind V.decode ev' of
            V.Become a -> TS.DTypeDef $ genTypeDef access n a
            V.Describe a -> TS.DInterface $ genInterface access n a
            V.BecomeWith _ -> panic "can't lower becomewith yet"
    Just $ TS.Module imports (TS.Exported <$> definitions)
  go n (V.Function ft) ev' = do
    let v = V.local (FId n) ft.dom
    go (n + 1) (V.appClo ft.cod v) (V.ebind (flip (V.app ft.variant) v) ev')
  go _ _ _ = Nothing

tableNameDoc :: TableName -> DDoc
tableNameDoc tn = concatWith (surround dot) (dpretty <$> (tn.realm : toList tn.path))

data FlatParams = FlatParams
  { paramVals :: Bwd TS.El
  , numParams :: Int
  }

allocParams :: TS.El -> SIR.Shape -> State FlatParams FLIR.Els
allocParams v = \case
  SIR.Tuple d -> do
    FLIR.Cons <$> mapWithKeyM (\x sh -> allocParams (TS.Proj v (mangle x)) sh) d
  SIR.Scalar _ -> state \p ->
    ( FLIR.Scalar (FLIR.Param (FId p.numParams))
    , p { paramVals = p.paramVals :> v, numParams = p.numParams + 1 }
    )
  SIR.Unstored -> pure FLIR.Erased

data TSEnv = TSEnv
  { tsLocals :: Bwd TS.El
  , usedNames :: Set.Set Name
  , flatParams :: FlatParams
  , flirLocals :: FLIR.Locals
  }

emptyTSEnv :: TSEnv
emptyTSEnv = TSEnv BwdNil Set.empty (FlatParams BwdNil 0) BwdNil

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

genQuery :: Access -> TSEnv -> SIR.Query -> TS.El
genQuery _access e q = do
  let ((v, mainProps), vars, auxProps) = FLIR.runFlatM $ do
        v <- FLIR.freshAt (BwdNil :> "result") q.shape
        mainprops <- FLIR.app e.flirLocals q.pred v
        pure (v, mainprops)
  let query = FLIR.Query vars (toList (mainProps <> auxProps))
  let flir = TS.String (undefined query)
  -- TODO: should make sure "result" is fresh
  let reconstruct =
        TS.Lam
          (TS.Binding "result" (TS.ListTy (TS.runtime Value)))
          (TS.Block [] (Just (reconstructEls e.flatParams v)))
  TS.New (TS.Const (TS.runtime Query)) [flir, reconstruct]

varName :: SIR.Abs a -> Set.Set Name -> Name
varName (SIR.Abs (Just x) _) xs = case Set.member x xs of
  True -> freshNameFor xs
  False -> x
varName _ xs = freshNameFor xs

genAbs :: Access -> TSEnv -> SIR.Abs (SIR.El l) -> (Name, TS.El)
genAbs access e (SIR.Abs mx body) = do
  let x = freshNameWithPref e.usedNames mx
  let e' = e
        { tsLocals = e.tsLocals :> TS.Var (mangle x)
        , usedNames = Set.insert x e.usedNames
        }
  (x, genEl access e' body)
genAbs access e (SIR.AbsConst body) = do
  let x = freshNameFor e.usedNames
  let e' = e { usedNames = Set.insert x e.usedNames }
  (x, genEl access e' body)

genEl :: Access -> TSEnv -> SIR.El l -> TS.El
genEl access e = \case
  SIR.LiftEl t -> genEl access e t
  SIR.Var i -> elemAt e.tsLocals i
  SIR.Single q -> TS.MethodCall (genQuery access e q) "single" []
  SIR.Proj t x -> TS.Proj (genEl access e t) (mangle x)
  SIR.Multi _ q -> TS.MethodCall (genQuery access e q) "multi" []
  SIR.Lam _dom abs -> do
    let (x, body) = genAbs access e abs
    TS.Lam
      (TS.Binding (mangle x) (TS.runtime Value))
      (TS.Block [] (Just body))
  SIR.Cons fields ->
    TS.Object [(mangle x, genEl access e t) | (x, t) <- toList fields]
  SIR.Lit l -> TS.Lit l
  SIR.Erased -> TS.Null

genRealmConstructor :: Access -> SIR.Realm -> TS.Constructor
genRealmConstructor access r = do
  let args = case access of
        View ->
          [ TS.Binding "store" (TS.runtime StoreHandle)
          ]
        Transaction ->
          [ TS.Binding "store" (TS.runtime StoreHandle)
          , TS.Binding "transaction" (TS.runtime TransactionHandle)
          ]
  let superCall = case extends access of
        Just _ -> [TS.Expr (TS.Call (TS.Var "super") [TS.Var "store"])]
        Nothing -> []
  let body =
        TS.Block
          (superCall ++ [TS.Assign (TS.QId ["this"] "root") (genEl access emptyTSEnv r.root)])
          Nothing
  TS.Constructor args body

genRealmClass :: Access -> SIR.Realm -> TS.Class
genRealmClass access r =
  TS.Class
    (fromShow access)
    Nothing
    (fromShow <$> extends access)
    [TS.Binding "root" (genTy access 0 r.rootType)]
    (genRealmConstructor access r)

genRealmModule :: [TS.Import] -> SIR.Realm -> TS.Module
genRealmModule imports r = do
  let classes = for accessLevels $ \access -> TS.DClass $ genRealmClass access r
  TS.Module imports (TS.Exported <$> classes)

render :: DDoc -> TL.Text
render = renderLazy . layoutPretty defaultLayoutOptions

writeModule :: FilePath -> Name -> TS.Module -> IO ()
writeModule outdir x mod = do
  let fn = outdir </> TS.idToString (mangle x) <> ".ts"
  let content = render $ asm mod
  TLIO.writeFile fn content

runtimeImport :: TS.Import
runtimeImport = TS.ImportQualified "runtime" "@coln-project/runtime"

forAccM :: (Monad m) => [b] -> a -> (a -> b -> m a) -> m a
forAccM bs init f = foldlM f init bs

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
