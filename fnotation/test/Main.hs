module Main where

import Test.FNotation.Golden (goldenTests)
import Test.FNotation.Property.Lexing (lexerProperties)
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
          [lexerProperties]
      ]
