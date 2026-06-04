module Main where

import ColnDo.Build
import ColnDo.Common
import ColnDo.Format
import ColnDo.Manual
import ColnDo.Self
import ColnDo.Site
import ColnDo.Test

import System.Directory (setCurrentDirectory)
import System.Environment (setEnv)

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
  StdoutTrim top <- cmd "git rev-parse --show-toplevel"
  -- Make sure that we are running from the root of the repository
  setCurrentDirectory top
  shakeArgs shakeOptions{shakeColor = True} allRules
