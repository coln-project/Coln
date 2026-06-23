-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Main (main) where

import Options.Applicative

import Coln.CLI.Check
import Coln.CLI.GenerateIR
import Coln.CLI.GenerateTS
import Coln.CLI.LanguageServer
import Coln.CLI.Options
import Coln.CLI.REPL

run :: Options -> IO ()
run (GenerateTS opts) = generateTS opts
run (GenerateIR opts) = generateIR opts
run (Check opts) = check opts
run Repl = runRepl
run LanguageServer = startServer

main :: IO ()
main = customExecParser (prefs showHelpOnEmpty) optionsInfo >>= run
