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
  { outputDir :: String
  , inputFile :: String
  }

generateTSOptions :: Parser GenerateTSOptions
generateTSOptions = GenerateTSOptions <$> inputFile <*> outputDir

data CheckOptions = CheckOptions
  { inputFile :: String
  }

checkOptions :: Parser CheckOptions
checkOptions = CheckOptions <$> inputFile

data Options = GenerateTS GenerateTSOptions | Check CheckOptions

options :: Parser Options
options = hsubparser
  ( command "generate-ts" (info (GenerateTS <$> generateTSOptions) (progDesc "generate typescript interface for Coln definitions"))
 <> command "check" (info (Check <$> checkOptions) (progDesc "check Coln definitions"))
  )

optionsInfo :: ParserInfo Options
optionsInfo = info (options <**> helper)
  ( fullDesc
 <> progDesc "Command line interface for Coln"
  )
