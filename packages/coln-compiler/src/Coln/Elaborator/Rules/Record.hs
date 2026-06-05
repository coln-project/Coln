{-# LANGUAGE TypeAbstractions #-}
module Coln.Elaborator.Rules.Record where

import Control.Monad (unless)
import Prettyprinter

import Coln.Common
import Coln.Core.Params
import Coln.Core.Value qualified as V
import Coln.Core.Syntax qualified as S
import Coln.Core.Memoed
import Coln.Core.Print
import Coln.Core.Evaluation
import Coln.Elaborator.Diagnostics
import Coln.Elaborator.Environment
import Coln.Elaborator.Judgment
import Coln.Report

data FieldDeclaration = FieldDeclaration
  { name :: Name
  , typ :: Judgment N
  }

formation :: (V.HasEvaluation c) => Span -> [FieldDeclaration] -> Judgment c
formation sp fieldTyps = descSyn "record type" sp $ \e -> do
  let go _ [] = pure (Set, [])
      go e' ((FieldDeclaration x ty_j):rest) = do
        ty <- typ ty_j e'
        (l, fieldTys) <- go (e' { scope = bind x ty.val e'.scope }) rest
        pure (maxLevel l (levelOf ty), (x, ty) : fieldTys)
  (l, fields) <- go (e { target = TargetAnonymous }) fieldTyps
  let rt = S.RecordType l (fromList fields)
  let u = case universeFor l of
        Nothing -> panic "record field too large"
        Just u' -> u'
  pure (V.U $ u, code $ record e.scope.locals rt)

data FieldSetting c = FieldSetting
  { name :: Name
  , setting :: Judgment c
  , span :: Span
  }

intro :: (V.HasEvaluation c) => Span -> [FieldSetting c] -> Judgment c
intro @c sp fieldSettings = Chk "record expression" sp $ \e a -> do
  let go :: V.Locals -> [(FieldSetting c, (Name, V.Locals -> V.Ty N))] -> IO [(Name, El c)]
      go _ [] = pure []
      go vs ((fs, (x, fieldTyC)):rest)
        | fs.name == x = do
            let fieldTy = fieldTyC vs
            let target' = projTarget e.target x
            m <- chk fs.setting (e { target = target' }) fieldTy
            let v = reflectTarget target' fieldTy m.val
            fields <- go (V.LSnoc vs v) rest
            pure ((x,m):fields)
        | otherwise = do
            let msg = "expected record field" <+> dpretty x <+> "got: " <+> dpretty fs.name
            failWith e.diagEnv fs.span MismatchedRecordField msg
  case V.behavior a of
    V.LikeRecord rt -> do
      let expectedLength = dictLength rt.fieldTypes
      let givenLength = length fieldSettings
      unless (expectedLength == givenLength) $ do
        let msg = "expected" <+> pretty expectedLength <+> "fields, got: " <+> pretty givenLength
        failWith e.diagEnv sp WrongNumberOfRecordFields msg
      fields <- go rt.capture (zip fieldSettings (toList rt.fieldTypes))
      pure $ cons (fromList fields)
    _ -> do
      let msg = "tried to check a record expression at a non-record type"
      failWith e.diagEnv sp CheckRecordAtNonRecordType msg

elim :: (V.HasEvaluation c) => Span -> Judgment N -> Name -> Judgment c
elim sp projectee x = elimSyn sp $ \e -> do
  (ty, eprojectee) <- syn "head of a field projection" projectee e
  case V.behavior ty of
    V.LikeRecord rt -> do
      unless (contains rt.fieldTypes x) $ do
        let msg = "no such field" <+> dpretty x <+> "in type" <+> prtIn e ty
        failWith e.diagEnv sp NoSuchField msg
      pure (V.projTy ty eprojectee.val x, proj eprojectee x)
    _ -> do
      let msg = "tried to project from a value that was not of a record type"
      failWith e.diagEnv sp ProjectionOfNonRecord msg
