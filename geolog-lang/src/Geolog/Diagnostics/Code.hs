module Geolog.Diagnostics.Code where

import Geolog.Common
import Geolog.Token qualified as T
import Prettyprinter

data Code
  = UnexpectedCharacter Char
  | UncontinuedQualifiedName
  | UnexpectedToken T.Kind T.Class
  | DefaultedPrec Name
  | IncompatiblePrecedences
  | DebugMisc (forall ann. Doc ann)

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

shortcode :: Code -> Doc ann
shortcode = \case
  -- codes 0-100 are special
  DebugMisc _ -> "D0000"
  -- codes 100-200 are for parsing
  UnexpectedCharacter _ -> "E0100"
  UncontinuedQualifiedName -> "W0101"
  -- codes 200-300 are for parsing
  UnexpectedToken _ _ -> "E0200"
  DefaultedPrec _ -> "W0201"
  IncompatiblePrecedences -> "E0202"

description :: Code -> Doc ann
description = \case
  UnexpectedCharacter c -> "Unexpected character" <+> "'" <> pretty c <> "'"
  UncontinuedQualifiedName -> "Expected another name segment after '/'"
  UnexpectedToken k cl ->
    "Unexpected token kind" <+> pretty k <> ", expected" <+> pretty cl
  DefaultedPrec x -> "Defaulted precedence of" <+> pretty x <+> "to the same as +"
  IncompatiblePrecedences -> "Incompatible precedences"
  DebugMisc m -> m

instance Pretty Code where
  pretty c = pretty (severity c) <> "[" <> shortcode c <> "]" <+> description c
