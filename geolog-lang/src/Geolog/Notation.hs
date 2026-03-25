module Geolog.Notation where

import FNotation
import FNotation.Tokens as K

lexConfig :: ConfTable Kind
lexConfig =
  confTableFromList
    [ ("sig", K.Block)
    , ("struct", K.Block)
    , ("theory", K.Decl)
    , ("def", K.Decl)
    , ("query", K.Decl)
    , ("let", K.Decl)
    , ("open", K.Decl)
    , ("import", K.Decl)
    , ("end", K.End)
    , ("Query", K.AKeyword)
    , ("Int", K.AKeyword)
    , ("String", K.AKeyword)
    , (":=", K.SKeyword)
    , ("=", K.SKeyword)
    , (":", K.SKeyword)
    , ("->", K.SKeyword)
    , ("~>", K.SKeyword)
    , ("=>", K.SKeyword)
    ]

parseConfig :: ConfTable Prec
parseConfig =
  confTableFromList
    [ (":=", Prec 10 AssocNon)
    , (":", Prec 20 AssocNon)
    , ("->", Prec 30 AssocR)
    , ("~>", Prec 30 AssocR)
    , ("=>", Prec 30 AssocR)
    , ("=", Prec 40 AssocNon)
    , ("+", Prec 50 AssocL)
    , ("-", Prec 50 AssocL)
    , ("*", Prec 60 AssocL)
    , ("/", Prec 60 AssocL)
    ]
