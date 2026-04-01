module Geolog.LSP.Buffer (LSPBufferInfo (..), LSPBufferT, LSPBuffer, analyzeBuffer, AnalyzedBuffer (..)) where

import Control.Exception (SomeException (..), evaluate)
import Control.Monad.Catch (MonadCatch, catch)
import Control.Monad.Identity (Identity)
import Control.Monad.Trans
import Control.Monad.Trans.Reader (ReaderT, ask)
import Data.Functor.Contravariant (contramap)
import Data.IORef (newIORef, readIORef)
import Data.Text qualified as T
import Data.Vector (Vector)
import Diagnostician qualified as D
import FNotation
import FNotation.Tokens (Token)
import Geolog.Core (GlobalEnv)
import Geolog.Diagnostics (GeologCode (..))
import Geolog.Elaborator (elabTop)
import Geolog.LSP.Types (LSPState)
import Geolog.Notation (lexConfig, parseConfig)
import Language.LSP.Protocol.Message (SMethod (..))
import Language.LSP.Protocol.Types (MessageType (..), NormalizedUri, ShowMessageParams (..), Uri)
import Language.LSP.Server (MonadLsp, sendNotification)
import Prelude hiding (lex)

data LSPBufferInfo = LSPBufferInfo
  { uri :: Uri
  , uriNormalised :: NormalizedUri
  , file :: D.File
  }

type LSPBufferT m = ReaderT LSPBufferInfo m

type LSPBuffer = LSPBufferT Identity

data AnalyzedBuffer = AnalyzedBuffer
  { tokens :: Maybe (Vector Token)
  , notations :: Maybe [FNotation.Ntn]
  , elaborated :: Maybe GlobalEnv
  , diagnostics :: [D.Diagnostic GeologCode]
  }

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
      Nothing -> pure $ AnalyzedBuffer Nothing Nothing Nothing []
      Just tokens ->
        reportCrash (FNotation.parse parseConfig (contramap ParserCode r) bufInfo.file tokens) "Parsing Error: " >>= \case
          Nothing -> pure $ AnalyzedBuffer (Just tokens) Nothing Nothing []
          Just notations ->
            reportCrash (elabTop (contramap ElaboratorCode r) bufInfo.file notations) "Elaboration Error: " >>= \case
              Nothing -> pure $ AnalyzedBuffer (Just tokens) (Just notations) Nothing []
              Just elaborated -> pure $ AnalyzedBuffer (Just tokens) (Just notations) (Just elaborated) []

  diagnostics <- liftIO $ readIORef diagRef

  pure $ analysis{diagnostics}
