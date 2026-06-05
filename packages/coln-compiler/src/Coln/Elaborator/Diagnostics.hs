module Coln.Elaborator.Diagnostics where

import Data.Map qualified as Map
import Diagnostician
import FNotation (LexerCode, ParserCode, lexerCodeTable, parserCodeTable)

import Coln.Common
import Coln.Report

data ReparseCode
  = UnexpectedNotation
  | UnexpectedTuple
  | UnexpectedLambda
  | UnexpectedField
  deriving (Eq, Ord)

reparseCodeTable :: Map ReparseCode CodeMeta
reparseCodeTable = Map.fromList
  [ (UnexpectedNotation, CodeMeta 0 SError Nothing)
  , (UnexpectedTuple, CodeMeta 1 SError Nothing)
  , (UnexpectedLambda, CodeMeta 2 SError Nothing)
  , (UnexpectedField, CodeMeta 3 SError Nothing)
  ]

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
  deriving (Eq, Ord)

elaboratorCodeTable :: Map ElaboratorCode CodeMeta
elaboratorCodeTable = Map.fromList
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
  ]

data ColnCode
  = LexerCode LexerCode
  | ParserCode ParserCode
  | ReparseCode ReparseCode
  | ElaboratorCode ElaboratorCode
  deriving (Eq, Ord)

colnCodeTable :: Map ColnCode CodeMeta
colnCodeTable = mconcat
  [ promoteCodeTable lexerCodeTable LexerCode 0
  , promoteCodeTable parserCodeTable ParserCode 100
  , promoteCodeTable reparseCodeTable ReparseCode 200
  , promoteCodeTable elaboratorCodeTable ElaboratorCode 300
  ]

instance Code ColnCode where
  codeMeta c = case Map.lookup c colnCodeTable of
    Just m -> m
    Nothing -> error "unregistered code"
