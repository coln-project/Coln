module Geolog.Diagnostics.Code where

import Data.Text (Text)
import Geolog.Common
import Geolog.Core
import Geolog.Token qualified as T
import Prettyprinter

data NotationCategory
  = Binding
  | Annot
  | Definition
  | Declaration
  | ApplicationPattern
  deriving (Show)

data Code
  = UnexpectedCharacter Char
  | UncontinuedQualifiedName
  | UnexpectedToken T.Kind T.Class
  | DefaultedPrec Name
  | IncompatiblePrecedences
  | DebugMisc (forall ann. Doc ann)
  | WrongLevel Text Level
  | MustChk Text
  | NotInScope QName
  | CannotApplyNonPi
  | TupleFoundAtUnexpectedType (forall ann. Doc ann)
  | ExpectedField QName QName
  | UnexpectedNotation Text
  | WrongNumberOfFields Int Int
  | OutOfUniverse Level Level
  | SynthesizedNonUniverse
  | NotConvertableEl (Doc Ann) (Doc Ann) (Doc Ann)
  | NotConvertableTy (Doc Ann) (Doc Ann) (Doc Ann)
  | Expected NotationCategory

data Severity = Debug | Info | Warning | Error

instance Pretty Severity where
  pretty = \case
    Debug -> "debug"
    Info -> "info"
    Warning -> "warning"
    Error -> "error"

severity :: Code -> Severity
severity = \case
  UnexpectedCharacter _ -> Error
  UncontinuedQualifiedName -> Warning
  UnexpectedToken _ _ -> Error
  DefaultedPrec _ -> Warning
  IncompatiblePrecedences -> Error
  DebugMisc _ -> Debug
  WrongLevel _ _ -> Error
  MustChk _ -> Error
  NotInScope _ -> Error
  CannotApplyNonPi -> Error
  TupleFoundAtUnexpectedType _ -> Error
  ExpectedField _ _ -> Error
  UnexpectedNotation _ -> Error
  WrongNumberOfFields _ _ -> Error
  OutOfUniverse _ _ -> Error
  SynthesizedNonUniverse -> Error
  NotConvertableEl _ _ _ -> Error
  NotConvertableTy _ _ _ -> Error
  Expected _ -> Error

shortcode :: Code -> Doc ann
shortcode = \case
  -- codes 0-99 are special
  DebugMisc _ -> "D0000"
  -- codes 100-199 are for parsing
  UnexpectedCharacter _ -> "E0100"
  UncontinuedQualifiedName -> "W0101"
  -- codes 200-299 are for parsing
  UnexpectedToken _ _ -> "E0200"
  DefaultedPrec _ -> "W0201"
  IncompatiblePrecedences -> "E0202"
  -- codes 300-399 are for elaboration
  WrongLevel _ _ -> "E0300"
  MustChk _ -> "E0301"
  NotInScope _ -> "E0302"
  CannotApplyNonPi -> "E0303"
  TupleFoundAtUnexpectedType _ -> "E0304"
  ExpectedField _ _ -> "E0305"
  UnexpectedNotation _ -> "E0306"
  WrongNumberOfFields _ _ -> "E0307"
  OutOfUniverse _ _ -> "E0308"
  SynthesizedNonUniverse -> "E0309"
  NotConvertableEl _ _ _ -> "E0310"
  NotConvertableTy _ _ _ -> "E0311"
  Expected _ -> "E0312"

description :: Code -> Doc Ann
description = \case
  UnexpectedCharacter c -> "Unexpected character" <+> "'" <> pretty c <> "'"
  UncontinuedQualifiedName -> "Expected another name segment after '/'"
  UnexpectedToken k cl ->
    "Unexpected token kind" <+> pretty k <> ", expected" <+> pretty cl
  DefaultedPrec x -> "Defaulted precedence of" <+> pretty x <+> "to the same as +"
  IncompatiblePrecedences -> "Incompatible precedences"
  DebugMisc m -> m
  WrongLevel t l -> pretty t <+> "not supported at level" <+> pretty l
  MustChk t -> pretty t <+> "only supported in checking position"
  NotInScope x -> pretty x <+> "is not in scope"
  CannotApplyNonPi -> "cannot apply member of a non-pi type"
  TupleFoundAtUnexpectedType a ->
    "unexpected tuple syntax found while checking at type" <+> a
  ExpectedField x x' -> "expected field" <+> pretty x <> ", got field" <+> pretty x'
  UnexpectedNotation s -> "unexpected notation for" <+> pretty s
  WrongNumberOfFields n m -> "wrong number of fields for record, expected:" <+> pretty n <> ", got" <+> pretty m
  OutOfUniverse l l' -> "cannot decode an element of universe level" <+> pretty l <+> "to a level" <+> pretty l' <+> "type"
  SynthesizedNonUniverse -> "synthesized a type that was a non-universe"
  NotConvertableEl d d' r -> "during conversion check, could not convert elements" <+> d <+> "and" <+> d' <> ". Reason:" <+> r
  NotConvertableTy d d' r -> "during conversion check, could not convert types" <+> d <+> "and" <+> d' <> ". Reason:" <+> r
  Expected c -> "expected notation for" <+> pretty (show c)

instance Pretty Code where
  pretty c = pretty (severity c) <> "[" <> shortcode c <> "]" <+> unAnnotate (description c)
