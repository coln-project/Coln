-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Test.FNotation.Spot where

import Data.Functor.Contravariant
import Data.IORef
import Data.Text (Text)
import Diagnostician
import FNotation
import Test.FNotation.Common
import Test.Tasty
import Test.Tasty.HUnit
import Prelude hiding (lex, read)

readErrorFree :: Text -> IO Bool
readErrorFree src = do
  ref <- newIORef ([] :: [Diagnostic TestCode])
  let r = pureReporter ref
  let f = newFile "<input>" src
  tokens <- lex lexConfig (contramap LexerCode r) f
  _ <- read readConfig (contramap ReaderCode r) f tokens
  errs <- readIORef ref
  pure $ null errs

spotTests :: TestTree
spotTests =
  testGroup
    "Spot checks"
    [ testCase "Statement without newline" $ do
        readErrorFree "def x := 2" @? "failed to read statement without newline"
    ]
