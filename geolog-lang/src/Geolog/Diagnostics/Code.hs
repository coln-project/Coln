module Geolog.Diagnostics.Code where

import Geolog.Token qualified as T
import Prettyprinter

data Code
  = UnexpectedCharacter Char
  | UnexpectedToken T.Kind T.Class

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
  UnexpectedToken _ _ -> Error

shortcode :: Code -> Doc ann
shortcode = \case
  -- codes 0-100 are for lexing
  UnexpectedCharacter _ -> "E0000"
  -- codes 100-200 are for parsing
  UnexpectedToken _ _ -> "E0100"

description :: Code -> Doc ann
description = \case
  UnexpectedCharacter c -> "Unexpected character" <+> "'" <> pretty c <> "'"
  UnexpectedToken k cl ->
    "Unexpected token" <+> pretty k <> ", expected " <+> pretty cl

instance Pretty Code where
  pretty c = pretty (severity c) <> "[" <> shortcode c <> "]" <+> description c
