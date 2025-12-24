module Geolog.Parser where

import Prelude hiding (lookup)
import Control.Monad.IO.Class
import Control.Monad.Reader (ReaderT, runReaderT)
import Control.Monad.Reader.Class
import Control.Monad.State.Class
import Control.Monad.State.Strict (StateT, evalStateT)
import Data.Vector qualified as V
import Geolog.Common
import Geolog.Diagnostics hiding (report)
import Geolog.Diagnostics qualified as D
import Geolog.Diagnostics.Code qualified as Code
import Geolog.Notation
import Geolog.Token qualified as T
import Lens.Micro.Platform hiding (at)
import Prettyprinter

data Env = Env
  { envTokens :: V.Vector T.Token,
    envFile :: File,
    envReporter :: Reporter
  }

makeFields ''Env

data State = State
  { statePos :: Int,
    stateGas :: Int
  }

makeFields ''State

newtype Parser a = Parser {runParser :: ReaderT Env (StateT State IO) a}
  deriving (Functor, Applicative, Monad, MonadIO, MonadState State, MonadReader Env)

report :: Span -> Code.Code -> Parser ()
report s c = do
  e <- ask
  let n = Note (Just (SourceLoc (e ^. file) s)) Nothing
  let d = Diagnostic c [n]
  liftIO $ D.report (e ^. reporter) d

debug :: (forall ann. Doc ann) -> Parser ()
debug m = do
  s <- curSpan
  report s (Code.DebugMisc m)

cur :: Parser T.Kind
cur = do
  st <- get
  if st ^. gas <= 0
    then error "out of gas"
    else pure ()
  ts <- view tokens
  pure $ T.tokenKind $ V.unsafeIndex ts (st ^. pos)

curSpan :: Parser Span
curSpan = do
  ts <- view tokens
  i <- use pos
  pure $ T.tokenSpan $ V.unsafeIndex ts i

curValue :: Parser T.TokenValue
curValue = do
  ts <- view tokens
  i <- use pos
  pure $ T.tokenValue $ V.unsafeIndex ts i

curName :: Parser Name
curName = curValue >>= \case
  T.VName x -> pure x
  _ -> error "expected token to be associated with a name"

curInt :: Parser Int
curInt = curValue >>= \case
  T.VInt x -> pure x
  _ -> error "expected token to be associated with an int"

at :: T.Kind -> Parser Bool
at k = (k ==) <$> cur

advance :: Parser ()
advance = do
  ts <- view tokens
  st <- get
  if st ^. pos < V.length ts - 1
    then do
      pos += 1
      gas .= 256
    else pure ()

eat :: T.Kind -> Parser Bool
eat k = at k >>= \case
  True -> advance >> pure True
  False -> pure False

reportUnexpected :: T.Kind -> T.Class -> Parser ()
reportUnexpected k c = do
  s <- curSpan
  report s (Code.UnexpectedToken k c)

expect :: T.Kind -> Parser ()
expect k = do
  k' <- cur
  if k == k'
    then advance
    else
      reportUnexpected k' (T.CSpecific k) >> pure ()

openingPos :: Parser Pos
openingPos = curSpan >>= \case
  Span s _ -> pure s

advanceClose :: Pos -> (Span -> Ntn) -> Parser Ntn
advanceClose s f = do
  (Span _ e) <- curSpan
  let n = f (Span s e)
  advance
  pure n


data Assoc = AssocL | AssocR | AssocNon
  deriving (Eq, Show)

data Prec = Prec
  { precBinding :: Int,
    precAssoc :: Assoc
  }
  deriving (Eq, Show)

makeFields ''Prec

precLe :: Prec -> Prec -> Maybe Bool
precLe (Prec b a) (Prec b' a')
  | b < b' = Just True
  | b > b' = Just False
  | otherwise = case (a, a') of
      (AssocL, AssocL) -> Just True
      (AssocR, AssocR) -> Just False
      _ -> Nothing

precs :: ConfTable Prec
precs =
  fromList
    [ (":", Prec 10 AssocNon),
      ("+", Prec 50 AssocL),
      ("-", Prec 50 AssocL),
      ("*", Prec 60 AssocL),
      ("/", Prec 60 AssocL)
    ]

argStarts :: V.Vector T.Kind
argStarts = V.fromList [T.LParen, T.AIdent, T.Field, T.Int]

argStart :: T.Kind -> Bool
argStart k = V.elem k argStarts

arg :: Parser Ntn
arg = do
  m <- openingPos
  cur >>= \case
    T.LParen -> do
      advance
      e <- expr
      expect T.RParen
      pure e
    T.AIdent -> do
      x <- curName
      advanceClose m $ Ident x
    T.Field -> do
      x <- curName
      advanceClose m $ Field x
    T.Int -> do
      i <- curInt
      advanceClose m $ Int i
    k -> do
      s <- curSpan
      report s (Code.UnexpectedToken k T.CExprStart)
      pure $ Error s

expr :: Parser Ntn
expr = arg >>= go (Prec 0 AssocNon) where
  go p lhs = do
    cur >>= \case
      k | k == T.SIdent || k == T.SKeyword -> do
            n <- curName
            s <- curSpan
            p' <- case lookup precs n of
              Just p' -> pure p'
              Nothing -> do
                report s (Code.DefaultedPrec n)
                pure $ Prec 50 AssocL
            case precLe p p' of
              Nothing -> do
                report s Code.IncompatiblePrecedences
                pure lhs
              Just False -> pure lhs
              Just True -> do
                advance
                rhs <- arg >>= go p'
                pure $ App2 lhs (Ident n s) rhs
      k | argStart k -> do
            a <- arg
            go p (App1 lhs a)
                
      _ -> pure lhs

parse :: Reporter -> File -> V.Vector T.Token -> IO Ntn
parse r f ts = do
  let s = State 0 256
  let e = Env ts f r
  evalStateT (runReaderT (runParser expr) e) s
