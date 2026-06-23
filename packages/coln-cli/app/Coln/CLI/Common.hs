-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.CLI.Common where

import Control.Exception
import Diagnostician
import System.IO (stdout)

import Coln.Core.Globals
import Coln.Frontend.Driver
import Data.Text qualified as T
import Data.Text.IO qualified as TIO

data ExitException = Exit
  deriving (Show)

instance Exception ExitException

catchExit :: IO () -> IO ()
catchExit action =
  try action >>= \case
    Right _ -> pure ()
    Left (_ :: ExitException) -> pure ()

loadFile :: FilePath -> IO Globals
loadFile fp =
  try (TIO.readFile fp) >>= \case
    Right contents -> compile fp contents
    Left (err :: IOError) -> do
      putStrLn $ "could not read file " ++ fp ++ " error: " ++ show err
      throw Exit

compile :: FilePath -> T.Text -> IO Globals
compile fp contents = do
  let reporter = fileReporter stdout
  let f = newFile fp contents
  topFromText reporter f
