module Coln.Elaborator.Rules.Equality where

import Coln.Common
import Coln.Core.Conversion
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

import Prettyprinter ((<+>))

formation :: Span -> Judgment N -> Judgment N -> Judgment c
formation sp lhs rhs = Typ sp $ \e -> do
  (lty, elhs) <- syn "equated value" lhs e
  (rty, erhs) <- syn "equated value" rhs e
  case defEq (shape e) lty rty of
    Left err -> do
      let msg = "types" <+> prtIn e lty <+> "and" <+> prtIn e rty <+> "of compared terms differ"
      let note = Just $ dpretty err
      failWithNote e.diagEnv sp TypeMismatch msg note
    Right _ -> case levelOf lty of
      Set -> pure $ equality $ S.EqualityType (fromVTy e.scope.len lty) elhs erhs
      _ -> do
        let msg = "equality requires data types, but" <+> prtIn e lty <+> "is a schema-level type"
        failWith e.diagEnv sp EqualityUnsupported msg
