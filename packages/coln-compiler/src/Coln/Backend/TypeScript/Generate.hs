module Coln.Backend.TypeScript.Generate where

import Control.Monad.State
import Control.Monad (forM_)
import Data.Map.Ordered qualified as OMap
import Data.Set (Set)
import Data.Set qualified as Set
import Data.String (IsString(..))
import Data.Text.Lazy qualified as TL
import Data.Text.Lazy.IO qualified as TLIO
import Data.Text.Lazy.Encoding qualified as TLE
import Prettyprinter
import Prettyprinter.Render.Text
import System.FilePath

import Coln.Backend.TypeScript.AST qualified as TS
import Coln.Backend.TypeScript.Assemble (asm)
import Coln.Backend.TypeScript.Params
import Coln.Common
import Coln.Core.Params
import Coln.Core.Realm
import Coln.Core.Syntax qualified as S
import Coln.Core.Value qualified as V
import Coln.Core.Globals
import Coln.Core.Readback
import Coln.Core.Evaluation

-- * DONE Create a file per theory which declares the interface to a
--   model of that theory.
--
-- * TODO Produce appropriate imports for each file
--
-- * TODO Create a file per realm which implements the universal model of
--   that realm in terms of 

mangle :: Name -> TS.Id
mangle x = TS.Id $ mconcat [pretty s <> "_slash_" | s <- x.init] <> pretty x.last

tyFromHead :: Access -> V.Head -> TS.Ty
tyFromHead access (V.GlobalVar x _) =
  TS.TyConst (TS.QId [mangle x] (fromString (show access)))
tyFromHead _ (V.LocalVar _) = TS.runtime Value

genTy :: Access -> CtxLen -> V.Ty N -> TS.Ty
genTy access n = \case
  V.U SetU -> TS.runtime (ColnSet access)
  V.Function ft -> do
    let v = V.local (FId n) ft.dom
    TS.Fun (TS.Binding (TS.Id "x") (TS.runtime Value)) (genTy access (n + 1) (V.appClo ft.cod v))
  V.EltOf _ _ -> TS.runtime Value
  V.Decode n -> tyFromHead access n.head
  _ -> error "not yet supported"

genInterface :: Access -> CtxLen -> V.Ty D -> TS.Interface
genInterface access n = \case
  V.Record rt -> do
    let name = fromString $ show access
    let extendsName = fromString . show <$> extends access
    TS.Interface name extendsName (go n rt.capture (toList rt.fieldTypes))
   where
    go _ _ [] = []
    go n' vs ((x, f):rest) = do
      let a = f vs
      let v = V.local (FId n') a
      let bnd = TS.Binding (mangle x) (genTy access n' a)
      bnd : go (n' + 1) (V.LSnoc vs v) rest

class TrackGlobals a where
  trackGlobals :: a -> State (Set Name) ()

-- instance TrackGlobals (f c) => TrackGlobals (S.Abs f c) where
--   trackGlobals abs = trackGlobals (absBody abs)

-- instance TrackGlobals a => TrackGlobals (Name, a) where
--   trackGlobals (_, t) = trackGlobals t

-- instance TrackGlobals (S.El c) where
--   trackGlobals = \case
--     S.LocalVar _ -> pure ()
--     S.GlobalVar x _ -> modify (Set.insert x)
--     S.Code a -> trackGlobals a
--     S.Lam dom body -> do
--       trackGlobals dom
--       trackGlobals body
--     S.App t0 t1 -> do
--       trackGlobals t0
--       trackGlobals t1
--     S.Cons ts -> mapM_ trackGlobals (toList ts)
--     S.Proj t _ -> trackGlobals t
--     S.Lit _ -> pure ()
--     S.Is t -> trackGlobals t
--     S.Lookup _ _ -> pure ()
  
-- instance TrackGlobals (S.Ty c) where
--   trackGlobals = \case
--     S.U _ -> pure ()
--     S.Decode t -> trackGlobals t
--     S.Function ft -> do
--       trackGlobals ft.dom
--       trackGlobals ft.cod
--     S.Record rt -> mapM_ trackGlobals (toList rt.fieldTypes)
--     S.Eq et -> do
--       trackGlobals et.lhs
--       trackGlobals et.rhs
--     S.BuiltinTy _ -> pure ()
--     S.IsTy a -> trackGlobals a
--     S.EltOf _ _ -> pure ()

genTypeDef :: Access -> CtxLen -> V.Ty N -> TS.TypeDef
genTypeDef access n a = TS.TypeDef (fromShow access) (genTy access n a)

genEntryModule :: V.Ty N -> V.Evaluation V.El D -> Maybe TS.Module
genEntryModule a ev = go 0 a ev
  where
    go :: CtxLen -> V.Ty N -> V.Evaluation V.El D -> Maybe TS.Module
    go n (V.U TheoryU) ev' = do
      let definitions = for accessLevels $ \access ->
            case V.ebind V.decode ev' of
              V.Become a -> TS.DTypeDef $ genTypeDef access n a
              V.Describe a -> TS.DInterface $ genInterface access n a 
      Just $ TS.Module [] (TS.Exported <$> definitions)
    go n (V.Function ft) ev' = do
      let v = V.local (FId n) ft.dom
      go (n + 1) (V.appClo ft.cod v) (V.ebind (flip V.app v ) ev')
    go _ _ _ = Nothing

genRealmClass :: Access -> Realm -> TS.Class
genRealmClass access _r = TS.Class
  (fromShow access)
  Nothing
  (fromShow <$> extends access)
  []
  (TS.Block [] Nothing)

genRealmModule :: Realm -> TS.Module
genRealmModule r = do
  let classes = for accessLevels $ \access ->
        TS.DClass $ genRealmClass access r
  TS.Module [] (TS.Exported <$> classes)

render :: DDoc -> TL.Text
render = renderLazy . layoutPretty defaultLayoutOptions

writeModule :: FilePath -> Name -> TS.Module -> IO ()
writeModule outdir x mod = do
  let fn = outdir </> (TS.idToString (mangle x) <> ".ts")
  let content = render $ asm mod
  TLIO.writeFile fn content

generate :: Globals -> FilePath -> IO ()
generate ge outdir = do
  forM_ (OMap.assocs ge.entries) $ \(x, e) -> do
    let ev = eval V.LNil e.syn
    let mod = genEntryModule e.ty ev
    maybe (pure ()) (writeModule outdir x) mod
  forM_ (OMap.assocs ge.realms) $ \(x, r) -> do
    let mod = genRealmModule r
    writeModule outdir x mod
