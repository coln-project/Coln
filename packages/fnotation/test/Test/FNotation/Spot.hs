module Test.FNotation.Spot where

import Prelude hiding (lex)
import FNotation
import Diagnostician
import Data.Text (Text)
import Test.FNotation.Common
import Data.IORef
import Data.Functor.Contravariant
import Test.Tasty
import Test.Tasty.HUnit

parseErrorFree :: Text -> IO Bool
parseErrorFree src = do
  ref <- newIORef ([] :: [Diagnostic TestCode])
  let r = pureReporter ref
  let f = newFile "<input>" src
  tokens <- lex lexConfig (contramap LexerCode r) f
  _ <- parse parseConfig (contramap ParserCode r) f tokens
  errs <- readIORef ref
  pure $ null errs

spotTests :: TestTree
spotTests = testGroup
  "Spot checks"
  [ testCase "Statement without newline" $ do
      parseErrorFree "def x := 2" @? "failed to parse statement without newline"
  ]
