module Geolog.Elaborator.Diagnostics where

import Geolog.Diagnostician.CodeMeta

data ElaboratorCode
  = FailedConversion
  | NotInScope
  | UnsupportedInPotentialMode
  | UnsupportedInKineticMode
  | ProjectionFromNonRecord
  | NoSuchField
  | ApplicationOfNonPi
  | MustChk
  | UnexpectedNotation
  | UnexpectedTuple
  | UnexpectedLambda
  | UnexpectedField
  | WrongNumberOfFields
  | WrongLevel
  deriving (Eq, Ord)

elaboratorCodeTable :: [(ElaboratorCode, Int, CodeMeta)]
elaboratorCodeTable =
  [ (FailedConversion, 0, CodeMeta Error Nothing),
    (NotInScope, 1, CodeMeta Error Nothing),
    (UnsupportedInPotentialMode, 2, CodeMeta Error Nothing),
    (UnsupportedInKineticMode, 3, CodeMeta Error Nothing),
    (ProjectionFromNonRecord, 4, CodeMeta Error Nothing),
    (NoSuchField, 5, CodeMeta Error Nothing),
    (ApplicationOfNonPi, 6, CodeMeta Error Nothing),
    (MustChk, 7, CodeMeta Error Nothing),
    (UnexpectedNotation, 8, CodeMeta Error Nothing),
    (UnexpectedTuple, 9, CodeMeta Error Nothing),
    (UnexpectedLambda, 10, CodeMeta Error Nothing),
    (UnexpectedField, 11, CodeMeta Error Nothing),
    (WrongNumberOfFields, 11, CodeMeta Error Nothing),
    (WrongLevel, 12, CodeMeta Error Nothing)
  ]
