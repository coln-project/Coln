module Coln.Report where

import Control.Exception (Exception, throwIO)
import Data.Functor.Contravariant (Contravariant(contramap))
import Diagnostician

data DiagnosticEnv c = DiagnosticEnv
  { reporter :: Reporter c
  , file :: File
  }

instance Contravariant DiagnosticEnv where
  contramap f d = d { reporter = contramap f d.reporter }

reportNote :: DiagnosticEnv c -> Span -> c -> DDoc -> Maybe DDoc -> IO ()
reportNote dc s c m n = do
  let n' = Note (Just $ SourceLoc dc.file s) n
  let d = Diagnostic c m [n']
  reportTo dc.reporter d

report :: DiagnosticEnv c -> Span -> c -> DDoc -> IO ()
report dc s c m = reportNote dc s c m Nothing

data FailException = GiveUp
  deriving (Show)

instance Exception FailException

failWithNote :: DiagnosticEnv c -> Span -> c -> DDoc -> Maybe DDoc -> IO a
failWithNote e s c m n = do
  reportNote e s c m n
  throwIO GiveUp

failWith :: DiagnosticEnv c -> Span -> c -> DDoc -> IO a
failWith e s c m = failWithNote e s c m Nothing
