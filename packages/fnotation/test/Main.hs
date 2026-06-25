-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Main where

import Test.FNotation.Golden (goldenTests)
import Test.FNotation.Property.Lexing (lexerProperties)
import Test.FNotation.Property.Parsing (parserProperties)
import Test.FNotation.Spot (spotTests)
import Test.Tasty

main :: IO ()
main = do
  golden <- goldenTests
  defaultMain $
    testGroup
      "FNotation"
      [ golden
      , testGroup
          "Property Tests"
          [ lexerProperties
          , parserProperties
          ]
      , spotTests
      ]
