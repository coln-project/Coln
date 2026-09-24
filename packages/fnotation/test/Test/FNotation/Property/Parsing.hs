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
import Prelude hiding (read)

-- | Property: the reader should not crash on any generated token stream.
readerProperties :: TestTree
readerProperties =
  testGroup
    "Reader properties"
    [ testProperty "reader does not crash on arbitrary tokens" \(FNTokens tokens) ->
        ioProperty do
          let f = newFile "<quickcheck>" ""
          r <- newReporter nullReporter
          ns <- read readConfig r f tokens
          length ns `seq` pure True
    ]
