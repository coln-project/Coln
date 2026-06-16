module Coln.Backend.TypeScript.Generate where

import Coln.Backend.TypeScript.AST qualified as TS
import Coln.Common
import Coln.Core.Params
import Coln.Core.Syntax qualified as S

-- What do we need to do?
--
-- We need to create a file per theory which declares the interface to a model of that theory.
--
-- We need to create a file per realm which declares the universal model of that realm.
--
-- This seems to imply that we actually do need a different namespace for realms and theories.

-- Theory interfaces

mangle :: Name -> TS.Id
mangle x = TS.Id $ mconcat [pretty s <> "_slash_" | s <- x.init] <> pretty x.last

absBody :: S.Abs a c -> a c
absBody (S.Abs _ t) = t
absBody (S.AbsConst t) = t

data Access
  = Readonly
  | ReadWrite

genTy :: Access -> S.Ty N -> TS.Ty
genTy access = \case
  S.U SetU -> case access of
    Readonly -> TS.runtime TS.ReadonlySet
    ReadWrite -> TS.runtime TS.ReadWriteSet
  S.Function ft ->
    TS.Fun (TS.Binding (TS.Id "x") (TS.runtime TS.Value)) (genTy access (absBody ft.cod))
  S.EltOf _ _ -> TS.runtime TS.Value
  S.Decode (S.GlobalVar x _) -> case access of
    Readonly -> TS.TyConst (TS.QId [mangle x] "Readonly")
    ReadWrite -> TS.TyConst (TS.QId [mangle x] "ReadWrite")
  _ -> error "not yet supported"

genInterface :: Access -> S.Ty D -> TS.Interface
genInterface access = \case
  S.Record rt -> do
    let name = case access of
          Readonly -> TS.Id "Readonly"
          ReadWrite -> TS.Id "ReadWrite"
    let extends = case access of
          Readonly -> Nothing
          ReadWrite -> Just $ TS.Id "Readonly"
    TS.Interface name extends (map doField (toList rt.fieldTypes))
   where
    doField (x, a) = TS.Binding (mangle x) (genTy access a)
  _ -> error "not yet supported"

genTop :: V.Ty N -> V.Evaluation V.El D -> TS.Module
genTop a v = go 0 a v
  where
    go i (V.U Theory) v' =

generate :: Globals -> FilePath -> IO ()
generate ge fp = do
