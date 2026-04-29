module Test.FNotation.Common where

import FNotation
import FNotation.Tokens qualified as K
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
