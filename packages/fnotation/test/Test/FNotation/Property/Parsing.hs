-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Test.FNotation.Property.Parsing where

import Diagnostician
import FNotation
import Test.FNotation.Common
import Test.FNotation.Property.Gen.Token
import Test.Tasty
import Test.Tasty.QuickCheck

-- | A reporter that silently discards all diagnostics.
nullReporter :: Reporter ParserCode
nullReporter = Reporter{reportIO = \_ -> pure ()}

-- | Property: the parser should not crash on any generated token stream.
parserProperties :: TestTree
parserProperties =
  testGroup
    "Parser properties"
    [ testProperty "parser does not crash on arbitrary tokens" \(FNTokens tokens) ->
        ioProperty do
          let f = newFile "<quickcheck>" ""
          ns <- parse parseConfig nullReporter f tokens
          length ns `seq` pure True
    ]
