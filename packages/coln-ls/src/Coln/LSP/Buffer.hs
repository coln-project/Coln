-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.LSP.Buffer (analyzeBuffer) where

import Coln.Diagnostics (ColnCode (..))
import Coln.Frontend.Driver (top)
import Coln.Frontend.Notation (lexConfig, parseConfig)
import Coln.LSP.Types (AnalyzedBuffer (..), LSPBufferInfo (..), LSPState)
import Coln.Report (DiagnosticEnv (DiagnosticEnv))
import Control.Exception (SomeException (..), evaluate)
import Control.Monad.Catch (MonadCatch, catch)
import Control.Monad.Trans
import Data.Functor.Contravariant (contramap)
import Data.IORef (newIORef, readIORef)
import Data.Text qualified as T
import Diagnostician qualified as D
import FNotation
import Language.LSP.Protocol.Message (SMethod (..))
import Language.LSP.Protocol.Types (MessageType (..), ShowMessageParams (..))
import Language.LSP.Server (MonadLsp, sendNotification)
import Prelude hiding (lex)

reportCrash :: (MonadIO m, MonadCatch m, MonadLsp LSPState m) => IO a -> T.Text -> m (Maybe a)
reportCrash m msg =
  catch
    (liftIO (m >>= evaluate . Just))
    ( \e@SomeException{} -> do
        sendNotification
          SMethod_WindowShowMessage
          ( ShowMessageParams MessageType_Error $
              msg
                <> (T.pack . show $ e)
          )
        pure Nothing
    )

analyzeBuffer :: (MonadIO m, MonadCatch m, MonadLsp LSPState m) => LSPBufferInfo -> m AnalyzedBuffer
analyzeBuffer bufInfo = do
  (r, diagRef) <- liftIO $ do
    diagRef <- newIORef ([] @(D.Diagnostic ColnCode))
    pure (D.pureReporter diagRef, diagRef)

  let buf tokens notations elaborated =
        AnalyzedBuffer
          { raw = bufInfo.file
          , tokens
          , notations
          , elaborated
          , diagnostics = []
          }

  analysis <- do
    reportCrash (FNotation.lex lexConfig (contramap LexerCode r) bufInfo.file) "Lexing Error: " >>= \case
      Nothing -> pure $ buf Nothing Nothing Nothing
      Just tokens ->
        reportCrash (FNotation.parse parseConfig (contramap ParserCode r) bufInfo.file tokens) "Parsing Error: " >>= \case
          Nothing -> pure $ buf (Just tokens) Nothing Nothing
          Just notations ->
            reportCrash (top (DiagnosticEnv r bufInfo.file) notations) "Elaboration Error: " >>= \case
              Nothing -> pure $ buf (Just tokens) (Just notations) Nothing
              Just elaborated -> pure $ buf (Just tokens) (Just notations) (Just elaborated)

  diagnostics <- liftIO $ readIORef diagRef

  pure $ analysis{diagnostics = diagnostics}
