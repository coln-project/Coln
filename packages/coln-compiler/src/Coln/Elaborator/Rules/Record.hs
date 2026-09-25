-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT
{-# LANGUAGE TypeAbstractions #-}

module Coln.Elaborator.Rules.Record where

import Coln.Common
import Coln.Core
import Coln.Core.Syntax qualified as S
import Coln.Core.Value qualified as V
import Coln.Elaborator.Debug
import Coln.Elaborator.Judgment
import Control.Monad (unless)

data FieldDeclaration
  = FieldDeclaration {name :: [Name], typ :: Typ N}
  | FieldDeclarationDebug DebugCommand

formation :: [FieldDeclaration] -> Typ D
formation fieldTyps = Typ $ \e -> do
  let go _ [] = pure (Level Set HUnit, [])
      go e' ((FieldDeclaration xs typ) : rest) = do
        ty <- typ.elab e'
        (l, fieldTys) <- go (e'{scope = bindMany xs ty.val Conjunctive e'.scope}) rest
        pure (maxLevel l (levelOf ty), (xs, ty) : fieldTys)
      go e' ((FieldDeclarationDebug ds) : rest) = do
        runDebug e' ds
        go e' rest
  (l, fields) <- go (e{target = TargetAnonymous}) fieldTyps
  let rt = S.RecordType l (fromList fields)
  pure $ record e.scope.locals rt

data FieldSetting c = FieldSetting
  { name :: Name
  , body :: Chk c
  , span :: Span
  }

intro :: (V.HasEvaluation c) => Span -> [FieldSetting c] -> Chk c
intro @c sp fieldSettings = Chk \e a -> do
  let goinner :: Level -> (Name, FieldSetting c) -> V.Ty N -> IO (Name, El c, V.El N)
      goinner lvl (x, fs) fieldTy
        | fs.name == x = do
            let target' = projTarget lvl e.target x
            m <- fs.body.elab (e{target = target'}) fieldTy
            let v = reflectTarget target' fieldTy m.val
            pure (x, m, v)
        | otherwise = do
            let msg = "expected record field" <+> dpretty x <+> "got: " <+> dpretty fs.name
            failWith e.diagEnv fs.span MismatchedRecordField msg
      go :: Level -> V.Locals -> [([Name], V.Locals -> V.Ty N)] -> [FieldSetting c] -> IO [(Name, El c)]
      go _ _ [] _ = pure []
      go lvl vs ((names, fieldTyC) : rest) settings = do
        let (groupSettings, restSettings) = splitAt (length names) settings
        let fieldTy = fieldTyC vs
        xmvs <- traverse (\n -> goinner lvl n fieldTy) (zip names groupSettings)
        let newvs = fmap (\(_, _, v) -> v) xmvs
        fields <- go lvl (V.LSnocChunk vs (fromList newvs)) rest restSettings
        let newxms = fmap (\(x, m, _) -> (x, m)) xmvs
        pure $ newxms ++ fields
  case V.behavior a of
    V.LikeRecord rt -> do
      let expectedLength = multiDictLength rt.fieldTypes
      let givenLength = length fieldSettings
      unless (expectedLength == givenLength) $ do
        let msg = "expected" <+> pretty expectedLength <+> "fields, got: " <+> pretty givenLength
        failWith e.diagEnv sp WrongNumberOfRecordFields msg
      fields <- go rt.level rt.capture (toList rt.fieldTypes) fieldSettings
      pure $ cons rt.level (fromList fields)
    _ -> do
      let msg = "tried to check a record expression at a non-record type"
      failWith e.diagEnv sp CheckRecordAtNonRecordType msg

elim :: Span -> Syn N -> Name -> Syn N
elim sp projectee x = Syn \e -> do
  (ty, eprojectee) <- projectee.elab e
  case V.behavior ty of
    V.LikeRecord rt -> do
      unless (contains rt.fieldTypes x) $ do
        let msg = "no such field" <+> dpretty x <+> "in type" <+> prtIn e ty
        failWith e.diagEnv sp NoSuchField msg
      pure (V.projTy ty eprojectee.val x, proj rt.level eprojectee x)
    _ -> do
      let msg = "tried to project from a value that was not of a record type"
      failWith e.diagEnv sp ProjectionOfNonRecord msg
