module Coln.CLI.Options where

import Options.Applicative

inputFile :: Parser String
inputFile = argument str
  ( metavar "INPUT_FILE"
 <> help "input file (.coln) for theories and realms"
  )

outputDir :: Parser String
outputDir = strOption
  ( short 'o'
 <> long "output-dir"
 <> metavar "OUTPUT_DIR"
 <> help "output directory for the generated typescript"
  )

data GenerateTSOptions = GenerateTSOptions
  { inputFile :: String
  , outputDir :: String
  }

generateTSOptions :: Parser GenerateTSOptions
generateTSOptions = GenerateTSOptions <$> inputFile <*> outputDir

data GenerateIROptions = GenerateIROptions
  { inputFile :: String
  , outputDir :: String
  }

generateIROptions :: Parser GenerateIROptions
generateIROptions = GenerateIROptions <$> inputFile <*> outputDir

data CheckOptions = CheckOptions
  { inputFile :: String
  }

checkOptions :: Parser CheckOptions
checkOptions = CheckOptions <$> inputFile

data Options = GenerateTS GenerateTSOptions | GenerateIR GenerateIROptions | Check CheckOptions | Repl | LanguageServer

options :: Parser Options
options = hsubparser
  ( command "generate-ts" (info (GenerateTS <$> generateTSOptions) (progDesc "generate typescript interface for Coln definitions"))
 <> command "generate-ir" (info (GenerateIR <$> generateIROptions) (progDesc "generate IR in JSON for Coln definitions"))
 <> command "check" (info (Check <$> checkOptions) (progDesc "check Coln definitions"))
 <> command "repl" (info (pure Repl) (progDesc "run the coln REPL"))
 <> command "language-server" (info (pure LanguageServer) (progDesc "run the coln language server"))
  )

optionsInfo :: ParserInfo Options
optionsInfo = info (options <**> helper)
  ( fullDesc
 <> progDesc "Command line interface for Coln"
  )
