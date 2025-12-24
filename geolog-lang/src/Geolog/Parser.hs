module Geolog.Parser where

import Control.Monad.IO.Class
import Control.Monad.Reader (ReaderT, runReaderT)
import Control.Monad.Reader.Class
import Control.Monad.State.Class
import Control.Monad.State.Strict (StateT, evalStateT)
import Data.Vector qualified as V
import Geolog.Common
import Geolog.Diagnostics
import Geolog.Diagnostics.Code qualified as Code
import Geolog.Notation
import Geolog.Token qualified as T
import Lens.Micro.Platform hiding (at)

data Env = Env
  { envTokens :: V.Vector T.Token
  , envFile :: File
  , envReporter :: Reporter
  }

makeFields ''Env

data State = State
  { statePos :: Int
  , stateGas :: Int
  }

makeFields ''State

newtype Parser a = Parser {runParser :: ReaderT Env (StateT State IO) a}
  deriving (Functor, Applicative, Monad, MonadIO, MonadState State, MonadReader Env)

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
curName =
  curValue >>= \case
    T.VName x -> pure x
    _ -> error "expected token to be associated with a name"

curInt :: Parser Int
curInt =
  curValue >>= \case
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
eat k =
  at k >>= \case
    True -> advance >> pure True
    False -> pure False

reportUnexpected :: T.Kind -> T.Class -> Parser ()
reportUnexpected k c = do
  e <- ask
  s <- curSpan
  let n = Note (Just (SourceLoc (e ^. file) s)) Nothing
  let d = Diagnostic (Code.UnexpectedToken k c) [n]
  liftIO $ report (e ^. reporter) d

expect :: T.Kind -> Parser ()
expect k = do
  k' <- cur
  if k == k'
    then advance
    else
      reportUnexpected k' (T.CSpecific k) >> pure ()

openingPos :: Parser Pos
openingPos =
  curSpan >>= \case
    Span s _ -> pure s

advanceClose :: Pos -> (Span -> Ntn) -> Parser Ntn
advanceClose s f = do
  (Span _ e) <- curSpan
  let n = f (Span s e)
  advance
  pure n

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
      reportUnexpected k T.CExprStart
      s <- curSpan
      advance
      pure $ Error s

expr :: Parser Ntn
expr = arg

parse :: Reporter -> File -> V.Vector T.Token -> IO Ntn
parse r f ts = do
  let s = State 0 256
  let e = Env ts f r
  evalStateT (runReaderT (runParser expr) e) s
