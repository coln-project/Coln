-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Test.FNotation.Common where

import Data.Map (Map)
import Data.Map qualified as Map
import Diagnostician
import FNotation
import FNotation.Kinds qualified as K
import Prelude hiding (lex)

lexConfig :: ConfTable Kind
lexConfig =
  confTableFromList
    [ ("sig", K.Block)
    , ("struct", K.Block)
    , ("sum", K.Block)
    , ("match", K.Block)
    , ("theory", K.Decl)
    , ("def", K.Decl)
    , ("type", K.Decl)
    , ("let", K.Decl)
    , ("open", K.Decl)
    , ("import", K.Decl)
    , ("inductive", K.Modifier)
    , ("export", K.Modifier)
    , ("end", K.End)
    , ("Type", K.AKeyword)
    , ("Int", K.AKeyword)
    , ("String", K.AKeyword)
    , (":=", K.SKeyword)
    , ("=", K.SKeyword)
    , (":", K.SKeyword)
    , ("->", K.SKeyword)
    , ("=>", K.SKeyword)
    ]

parseConfig :: ConfTable Prec
parseConfig =
  confTableFromList
    [ (":=", Prec 10 AssocNon)
    , (":", Prec 20 AssocNon)
    , ("->", Prec 30 AssocR)
    , ("=>", Prec 30 AssocR)
    , ("=", Prec 40 AssocNon)
    , ("+", Prec 50 AssocL)
    , ("-", Prec 50 AssocL)
    , ("*", Prec 60 AssocL)
    , ("/", Prec 60 AssocL)
    ]

data TestCode = LexerCode LexerCode | ParserCode ParserCode
  deriving (Eq, Ord)

codeTable :: Map TestCode CodeMeta
codeTable =
  mconcat
    [ promoteCodeTable lexerCodeTable LexerCode 0
    , promoteCodeTable parserCodeTable ParserCode 100
    ]

instance Code TestCode where
  codeMeta c = case Map.lookup c codeTable of
    Just m -> m
    Nothing -> error "unregistered code"
