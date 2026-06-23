-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Diagnostics where

import Coln.Elaborator.Diagnostics
import Coln.Frontend.Diagnostics
import Data.Map (Map)
import Data.Map qualified as Map
import Diagnostician
import FNotation

data ColnCode
  = LexerCode LexerCode
  | ParserCode ParserCode
  | FrontendCode FrontendCode
  | ElaboratorCode ElaboratorCode
  deriving (Eq, Ord)

colnCodeTable :: Map ColnCode CodeMeta
colnCodeTable =
  mconcat
    [ promoteCodeTable lexerCodeTable LexerCode 0
    , promoteCodeTable parserCodeTable ParserCode 100
    , promoteCodeTable frontendCodeTable FrontendCode 200
    , promoteCodeTable elaboratorCodeTable ElaboratorCode 300
    ]

instance Code ColnCode where
  codeMeta c = case Map.lookup c colnCodeTable of
    Just m -> m
    Nothing -> error "unregistered code"
