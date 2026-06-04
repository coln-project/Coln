module Coln.Diagnostics where

import Data.Map (Map)
import Data.Map qualified as Map
import Diagnostician
import FNotation
import Coln.Elaborator (ElaboratorCode, elaboratorCodeTable)

data ColnCode
  = LexerCode LexerCode
  | ParserCode ParserCode
  | ElaboratorCode ElaboratorCode
  deriving (Eq, Ord)

geologCodeTable :: Map ColnCode CodeMeta
geologCodeTable =
  mconcat
    [ promoteCodeTable lexerCodeTable LexerCode 0
    , promoteCodeTable parserCodeTable ParserCode 100
    , promoteCodeTable elaboratorCodeTable ElaboratorCode 200
    ]

instance Code ColnCode where
  codeMeta c = case Map.lookup c geologCodeTable of
    Just m -> m
    Nothing -> error "unregistered code"
