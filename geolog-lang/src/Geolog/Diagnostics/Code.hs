module Geolog.Diagnostics.Code where

import Data.Text (Text)
import Geolog.Common
import Geolog.Core
import Geolog.Token qualified as T
import Prettyprinter

data Code
  = UnexpectedCharacter Char
  | UncontinuedQualifiedName
  | UnexpectedToken T.Kind T.Class
  | DefaultedPrec Name
  | IncompatiblePrecedences
  | DebugMisc (forall ann. Doc ann)
  | WrongLevel Text Level

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

description :: Code -> Doc ann
description = \case
  UnexpectedCharacter c -> "Unexpected character" <+> "'" <> pretty c <> "'"
  UncontinuedQualifiedName -> "Expected another name segment after '/'"
  UnexpectedToken k cl ->
    "Unexpected token kind" <+> pretty k <> ", expected" <+> pretty cl
  DefaultedPrec x -> "Defaulted precedence of" <+> pretty x <+> "to the same as +"
  IncompatiblePrecedences -> "Incompatible precedences"
  DebugMisc m -> m
  WrongLevel t l -> pretty t <+> "not supported at level" <+> pretty l

instance Pretty Code where
  pretty c = pretty (severity c) <> "[" <> shortcode c <> "]" <+> description c
