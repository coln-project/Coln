-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Test.FNotation.Property.Lexing where

import Data.Vector qualified as V
import Diagnostician
import FNotation
import Test.FNotation.Common
import Test.FNotation.Property.Gen.Source
import Test.Tasty
import Test.Tasty.QuickCheck
import Prelude hiding (lex)

-- | Property: the lexer should not crash on any generated source text.
lexerProperties :: TestTree
lexerProperties =
  testGroup
    "Lexer properties"
    [ testProperty "lexer does not crash on arbitrary source" \(FNSource src) ->
        ioProperty do
          let f = newFile "<quickcheck>" src
          r <- newReporter nullReporter
          tokens <- lex lexConfig r f
          pure $ V.length tokens `seq` True
    ]
