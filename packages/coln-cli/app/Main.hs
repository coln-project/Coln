module Main (main) where

import Options.Applicative

import Coln.CLI.Options
import Coln.CLI.Check
import Coln.CLI.GenerateTS

run :: Options -> IO ()
run (Check opts) = check opts
run (GenerateTS opts) = generateTS opts

main :: IO ()
main = execParser optionsInfo >>= run
