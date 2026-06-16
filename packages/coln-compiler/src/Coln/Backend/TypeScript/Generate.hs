module Coln.Backend.TypeScript.Generate where

import Control.Monad.State
import Control.Monad (forM_)
import Data.Set (Set)
import Data.Set qualified as Set
import Data.Text.Lazy qualified as TL
import Data.Text.Lazy.IO qualified as TLIO
import Data.Text.Lazy.Encoding qualified as TLE
import Prettyprinter
import Prettyprinter.Render.Text
import System.FilePath

import Coln.Backend.TypeScript.AST qualified as TS
import Coln.Backend.TypeScript.Assemble (asm)
import Coln.Common
import Coln.Core.Params
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

data Access
  = Readonly
  | ReadWrite

tyFromHead :: Access -> V.Head -> TS.Ty
tyFromHead access (V.GlobalVar x _) = case access of
  Readonly -> TS.TyConst (TS.QId [mangle x] "Readonly")
  ReadWrite -> TS.TyConst (TS.QId [mangle x] "ReadWrite")
tyFromHead _ (V.LocalVar _) = TS.runtime TS.Value

genTy :: Access -> CtxLen -> V.Ty N -> TS.Ty
genTy access n = \case
  V.U SetU -> case access of
    Readonly -> TS.runtime TS.ReadonlySet
    ReadWrite -> TS.runtime TS.ReadWriteSet
  V.Function ft -> do
    let v = V.local (FId n) ft.dom
    TS.Fun (TS.Binding (TS.Id "x") (TS.runtime TS.Value)) (genTy access (n + 1) (V.appClo ft.cod v))
  V.EltOf _ _ -> TS.runtime TS.Value
  V.Decode n -> tyFromHead access n.head
  _ -> error "not yet supported"

genInterface :: Access -> CtxLen -> V.Ty D -> TS.Interface
genInterface access n = \case
  V.Record rt -> do
    let name = case access of
          Readonly -> TS.Id "Readonly"
          ReadWrite -> TS.Id "ReadWrite"
    let extends = case access of
          Readonly -> Nothing
          ReadWrite -> Just $ TS.Id "Readonly"
    TS.Interface name extends (go n rt.capture (toList rt.fieldTypes))
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

genTop :: V.Ty N -> V.Evaluation V.El D -> TS.Module
genTop a ev = go 0 a ev
  where
    go :: CtxLen -> V.Ty N -> V.Evaluation V.El D -> TS.Module
    go n (V.U TheoryU) ev' = case V.ebind V.decode ev' of
      V.Become a -> do
        let readonly = TS.TypeDef "Readonly" (genTy Readonly n a)
        let readwrite = TS.TypeDef "ReadWrite" (genTy ReadWrite n a)
        TS.Module [] $ TS.Exported <$> [TS.DTypeDef readonly, TS.DTypeDef readwrite]
      V.Describe a -> do
        let readonly = genInterface Readonly n a
        let readwrite = genInterface ReadWrite n a
        TS.Module [] $ TS.Exported <$> [TS.DInterface readonly, TS.DInterface readwrite]
    go n (V.Function ft) ev' = do
      let v = V.local (FId n) ft.dom
      go (n + 1) (V.appClo ft.cod v) (V.ebind (flip V.app v ) ev')

render :: DDoc -> TL.Text
render = renderLazy . layoutPretty defaultLayoutOptions

generate :: Globals -> FilePath -> IO ()
generate ge outdir = do
  forM_ (toList ge) $ \(x, e) -> do
    let ev = eval V.LNil e.syn
    let mod = genTop e.ty ev
    let fn = outdir </> (TS.idToString (mangle x) <> ".ts")
    let content = render $ asm mod
    putStrLn fn
    TLIO.writeFile fn content
