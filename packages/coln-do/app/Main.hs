-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Main where

import ColnDo.Build
import ColnDo.Common hiding (getEnv)
import ColnDo.Format
import ColnDo.Manual
import ColnDo.Self
import ColnDo.Site
import ColnDo.Test

import Control.Monad (when)
import Data.Maybe (isNothing)
import System.Directory (setCurrentDirectory)
import System.Environment (lookupEnv, setEnv)

allRules :: Rules ()
allRules = do
  buildRules
  formatRules
  manualRules
  selfRules
  siteRules
  testRules

main :: IO ()
main = do
  setEnv "LANG" "en_US.UTF-8"
  rootCheck <- isNothing <$> lookupEnv "DONT_CHECK_ROOT"
  when rootCheck $ do
    StdoutTrim top <- cmd "git rev-parse --show-toplevel"
    -- Make sure that we are running from the root of the repository
    setCurrentDirectory top
  shakeArgs shakeOptions{shakeColor = True} allRules
