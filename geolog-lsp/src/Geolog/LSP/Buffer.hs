module Geolog.LSP.Buffer (analyzeBuffer) where

import Control.Exception (SomeException (..), evaluate)
import Control.Monad.Catch (MonadCatch, catch)
import Control.Monad.Trans
import Control.Monad.Trans.Reader (ask)
import Data.Functor.Contravariant (contramap)
import Data.IORef (newIORef, readIORef)
import Data.Text qualified as T
import Diagnostician qualified as D
import FNotation
import Geolog.Diagnostics (GeologCode (..))
import Geolog.Elaborator (elabTop)
import Geolog.LSP.Types (AnalyzedBuffer (..), LSPBufferInfo (..), LSPBufferT, LSPState)
import Geolog.Notation (lexConfig, parseConfig)
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

analyzeBuffer :: (MonadIO m, MonadCatch m, MonadLsp LSPState m) => LSPBufferT m AnalyzedBuffer
analyzeBuffer = do
  bufInfo <- ask

  (r, diagRef) <- liftIO $ do
    diagRef <- newIORef ([] @(D.Diagnostic GeologCode))
    pure (D.pureReporter diagRef, diagRef)

  analysis <- do
    reportCrash (FNotation.lex lexConfig (contramap LexerCode r) bufInfo.file) "Lexing Error: " >>= \case
      Nothing -> pure $ AnalyzedBuffer bufInfo.file Nothing Nothing Nothing []
      Just tokens ->
        reportCrash (FNotation.parse parseConfig (contramap ParserCode r) bufInfo.file tokens) "Parsing Error: " >>= \case
          Nothing -> pure $ AnalyzedBuffer bufInfo.file (Just tokens) Nothing Nothing []
          Just notations ->
            reportCrash (elabTop (contramap ElaboratorCode r) bufInfo.file notations) "Elaboration Error: " >>= \case
              Nothing -> pure $ AnalyzedBuffer bufInfo.file (Just tokens) (Just notations) Nothing []
              Just elaborated -> pure $ AnalyzedBuffer bufInfo.file (Just tokens) (Just notations) (Just elaborated) []

  diagnostics <- liftIO $ readIORef diagRef

  pure $ analysis{diagnostics = diagnostics}
