-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Frontend.Diagnostics where

import Coln.Common
import Data.Map qualified as Map

data ParserCode
  = UnexpectedNotation
  | UnexpectedTuple
  | UnexpectedLambda
  | UnexpectedField
  | UnexpectedDescriptive
  | UnknownCommand
  deriving (Eq, Ord)

parserCodeTable :: Map ParserCode CodeMeta
parserCodeTable =
  Map.fromList
    [ (UnexpectedNotation, CodeMeta 0 SError Nothing)
    , (UnexpectedTuple, CodeMeta 1 SError Nothing)
    , (UnexpectedLambda, CodeMeta 2 SError Nothing)
    , (UnexpectedField, CodeMeta 3 SError Nothing)
    , (UnexpectedDescriptive, CodeMeta 4 SError Nothing)
    , (UnknownCommand, CodeMeta 5 SError Nothing)
    ]
