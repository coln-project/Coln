-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Frontend.Notation where

import FNotation
import FNotation.Kinds as K

lexConfig :: ConfTable Kind
lexConfig =
  confTableFromList
    [ ("sig", K.Block)
    , ("struct", K.Block)
    , ("theory", K.Decl)
    , ("realm", K.Block)
    , ("def", K.Decl)
    , ("let", K.Decl)
    , ("open", K.Decl)
    , ("import", K.Decl)
    , ("showtypeb", K.Decl)
    , ("showtype", K.Decl)
    , ("showlevel", K.Decl)
    , ("expand", K.Decl)
    , ("end", K.End)
    , ("Set", K.AKeyword)
    , ("Prop", K.AKeyword)
    , ("Int", K.AKeyword)
    , ("String", K.AKeyword)
    , ("Inductive", K.AKeyword)
    , ("pure", K.AKeyword)
    , ("init", K.AKeyword)
    , (":=", K.SKeyword)
    , ("=", K.SKeyword)
    , (":", K.SKeyword)
    , ("*:", K.SKeyword)
    , ("->", K.SKeyword)
    , ("*->", K.SKeyword)
    , ("=>", K.SKeyword)
    , ("@", K.SKeyword)
    ]

readConfig :: ConfTable Prec
readConfig =
  confTableFromList
    [ ("@", Prec 10 AssocNon)
    , (":=", Prec 10 AssocNon)
    , (":", Prec 20 AssocNon)
    , ("*:", Prec 20 AssocNon)
    , ("->", Prec 30 AssocR)
    , ("*->", Prec 30 AssocR)
    , ("=>", Prec 30 AssocR)
    , ("=", Prec 40 AssocNon)
    , ("+", Prec 50 AssocL)
    , ("-", Prec 50 AssocL)
    , ("*", Prec 60 AssocL)
    , ("/", Prec 60 AssocL)
    ]
