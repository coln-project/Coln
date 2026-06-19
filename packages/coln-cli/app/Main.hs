module Main (main) where

import Options.Applicative

import Coln.CLI.Options
import Coln.CLI.Check
import Coln.CLI.GenerateTS
import Coln.CLI.GenerateIR

run :: Options -> IO ()
run (GenerateTS opts) = generateTS opts
run (GenerateIR opts) = generateIR opts
run (Check opts) = check opts

main :: IO ()
main = execParser optionsInfo >>= run
