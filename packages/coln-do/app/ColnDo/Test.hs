-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module ColnDo.Test where

import ColnDo.Common

testRules :: Rules ()
testRules = do
  phony "test-haskell" $ do
    cmd_ "cabal test all"

  phony "test-rust" $ do
    cmd_ "cargo test"

  phony "test" $ do
    need ["test-haskell"]
