module Main (main) where

import Options.Applicative

import Coln.CLI.Options
import Coln.CLI.Check
import Coln.CLI.GenerateTS
import Coln.CLI.GenerateIR
import Coln.CLI.REPL

run :: Options -> IO ()
run (GenerateTS opts) = generateTS opts
run (GenerateIR opts) = generateIR opts
run (Check opts) = check opts
run Repl = runRepl

main :: IO ()
main = execParser optionsInfo >>= run
