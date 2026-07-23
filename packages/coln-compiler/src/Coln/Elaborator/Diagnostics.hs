-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Elaborator.Diagnostics (
  ElaboratorCode (..),
  elaboratorCodeTable,
) where

import Data.Map qualified as Map
import Diagnostician

import Coln.Common

data ElaboratorCode
  = TypeMismatch
  | RequiresName
  | ProjectionOfNonRecord
  | NoSuchField
  | ApplicationOfNonFunction
  | AnnotationRequired
  | WrongNumberOfRecordFields
  | TypeTooLarge
  | EqualityUnsupported
  | TypeAtNonUniverse
  | CheckLambdaAtNonFunctionType
  | FunctionDomainTooLarge
  | CheckRecordAtNonRecordType
  | MismatchedRecordField
  | VariableNotInScope
  | DebugMisc
  | InductiveInConjunctive
  | InitInConjunctive
  deriving (Eq, Ord)

elaboratorCodeTable :: Map ElaboratorCode CodeMeta
elaboratorCodeTable =
  Map.fromList
    [ (TypeMismatch, CodeMeta 0 SError Nothing)
    , (RequiresName, CodeMeta 1 SError Nothing)
    , (ProjectionOfNonRecord, CodeMeta 2 SError Nothing)
    , (NoSuchField, CodeMeta 3 SError Nothing)
    , (ApplicationOfNonFunction, CodeMeta 4 SError Nothing)
    , (AnnotationRequired, CodeMeta 5 SError Nothing)
    , (WrongNumberOfRecordFields, CodeMeta 6 SError Nothing)
    , (TypeTooLarge, CodeMeta 7 SError Nothing)
    , (EqualityUnsupported, CodeMeta 8 SError Nothing)
    , (TypeAtNonUniverse, CodeMeta 9 SError Nothing)
    , (CheckLambdaAtNonFunctionType, CodeMeta 10 SError Nothing)
    , (FunctionDomainTooLarge, CodeMeta 11 SError Nothing)
    , (CheckRecordAtNonRecordType, CodeMeta 12 SError Nothing)
    , (MismatchedRecordField, CodeMeta 13 SError Nothing)
    , (VariableNotInScope, CodeMeta 14 SError Nothing)
    , (DebugMisc, CodeMeta 15 SDebug Nothing)
    , (InductiveInConjunctive, CodeMeta 16 SError Nothing)
    , (InitInConjunctive, CodeMeta 17 SError Nothing)
    ]
