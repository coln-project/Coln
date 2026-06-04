module Geolog.Elaborator.Diagnostics where

import Data.Map qualified as Map
import Diagnostician

import Geolog.Common
import Geolog.Report

data ReparseCode
  = NotInScope
  | UnexpectedNotation
  | UnexpectedTuple
  | UnexpectedLambda
  | UnexpectedField
  deriving (Eq, Ord)

reparseCodeTable :: Map ReparseCode CodeMeta
reparseCodeTable = Map.fromList
  [ (NotInScope, CodeMeta 0 SError Nothing)
  , (UnexpectedNotation, CodeMeta 1 SError Nothing)
  , (UnexpectedTuple, CodeMeta 2 SError Nothing)
  , (UnexpectedLambda, CodeMeta 3 SError Nothing)
  , (UnexpectedField, CodeMeta 4 SError Nothing)
  ]

data ElaboratorCode
  = TypeMismatch
  | RequiresName
  | ProjectionFromNonRecord
  | NoSuchField
  | ApplicationOfNonFunction
  | AnnotationRequired
  | WrongNumberOfFields
  | TypeTooLarge
  | EqualityUnsupported
  | TypeAtNonUniverse
  deriving (Eq, Ord)

elaboratorCodeTable :: Map ElaboratorCode CodeMeta
elaboratorCodeTable = Map.fromList
  [ (TypeMismatch, CodeMeta 0 SError Nothing)
  , (RequiresName, CodeMeta 1 SError Nothing)
  , (ProjectionFromNonRecord, CodeMeta 2 SError Nothing)
  , (NoSuchField, CodeMeta 3 SError Nothing)
  , (ApplicationOfNonFunction, CodeMeta 4 SError Nothing)
  , (AnnotationRequired, CodeMeta 5 SError Nothing)
  , (WrongNumberOfFields, CodeMeta 6 SError Nothing)
  , (TypeTooLarge, CodeMeta 7 SError Nothing)
  , (EqualityUnsupported, CodeMeta 8 SError Nothing)
  , (TypeAtNonUniverse, CodeMeta 9 SError Nothing)
  ]
