module Geolog.Diagnostics where

import Data.Map (Map)
import Data.Map qualified as Map
import Diagnostician
import FNotation
import Geolog.Elaborator (ElaboratorCode, elaboratorCodeTable)

data GeologCode
  = LexerCode LexerCode
  | ParserCode ParserCode
  | ElaboratorCode ElaboratorCode
  deriving (Eq, Ord)

geologCodeTable :: Map GeologCode CodeMeta
geologCodeTable =
  mconcat
    [ promoteCodeTable lexerCodeTable LexerCode 0
    , promoteCodeTable parserCodeTable ParserCode 100
    , promoteCodeTable elaboratorCodeTable ElaboratorCode 200
    ]

instance Code GeologCode where
  codeMeta c = case Map.lookup c geologCodeTable of
    Just m -> m
    Nothing -> error "unregistered code"
