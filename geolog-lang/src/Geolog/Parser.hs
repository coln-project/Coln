module Geolog.Parser (Assoc (..), Prec (..), precLe, parse) where

import Control.Monad.IO.Class
import Control.Monad.Reader (ReaderT, runReaderT)
import Control.Monad.Reader.Class
import Control.Monad.State.Class
import Control.Monad.State.Strict (StateT, evalStateT)
import Data.Vector qualified as V
import Geolog.Common
import Geolog.Diagnostician
import Geolog.Notation
import Geolog.Parser.Diagnostics
import Geolog.Token qualified as T
import Lens.Micro.Platform hiding (at)
import Prettyprinter
import Prelude hiding (lookup)

-- Parser monad
--------------------------------------------------------------------------------

data Env = Env
  { envTokens :: V.Vector T.Token
  , envFile :: File
  , envReporter :: Reporter
  }

makeFields ''Env

data State = State
  { statePos :: Int
  , stateGas :: Int
  , stateSkipNewlines :: Bool
  }

makeFields ''State

newtype Parser a = Parser {runParser :: ReaderT Env (StateT State IO) a}
  deriving (Functor, Applicative, Monad, MonadIO, MonadState State, MonadReader Env)

-- Parsing utilities
--------------------------------------------------------------------------------

report :: Span -> ParserCode -> ADoc -> Parser ()
report s c m = do
  e <- ask
  let n = Note (Just (SourceLoc (e ^. file) s)) Nothing
  let d = Diagnostic (ParserCode c) m [n]
  liftIO $ reportIO (e ^. reporter) d

debug :: (forall ann. Doc ann) -> Parser ()
debug m = do
  s <- curSpan
  e <- ask
  let n = Note (Just (SourceLoc (e ^. file) s)) Nothing
  let d = Diagnostic DebugMisc m [n]
  liftIO $ reportIO (e ^. reporter) d

cur :: Parser T.Kind
cur = do
  st <- get
  if st ^. gas <= 0
    then error "out of gas"
    else gas -= 1
  ts <- view tokens
  let f i = (V.unsafeIndex ts i).kind
  if st ^. skipNewlines
    then go f
    else pure $ f (st ^. pos)
 where
  go :: (Int -> T.Kind) -> Parser T.Kind
  go f =
    f <$> (use pos) >>= \case
      T.Nl -> do
        pos += 1
        go f
      k -> pure k

startSkipNewlines :: Parser Bool
startSkipNewlines = do
  st <- get
  skipNewlines .= True
  pure $ st ^. skipNewlines

endSkipNewlines :: Bool -> Parser ()
endSkipNewlines b = skipNewlines .= b

curSpan :: Parser Span
curSpan = do
  ts <- view tokens
  i <- use pos
  pure $ (V.unsafeIndex ts i).span

curValue :: Parser T.TokenValue
curValue = do
  ts <- view tokens
  i <- use pos
  pure $ (V.unsafeIndex ts i).value

curName :: Parser Name
curName =
  curValue >>= \case
    T.VName x -> pure x
    _ -> error "expected token to be associated with a name"

curQName :: Parser QName
curQName =
  curValue >>= \case
    T.VQName x -> pure x
    _ -> error "expected token to be associated with a qualified name"

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
  s <- curSpan
  report s UnexpectedToken $
    "Unexpected token kind" <+> pretty k <> ", expected" <+> pretty c

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

close :: Pos -> (Span -> Ntn) -> Parser Ntn
close s f = do
  (Span _ e) <- curSpan
  pure $ f (Span s e)

advanceClose :: Pos -> (Span -> Ntn) -> Parser Ntn
advanceClose s f = do
  n <- close s f
  advance
  pure n

-- Precedence/associativity
--------------------------------------------------------------------------------

-- TODO: understand how to do associativity via assigning a precedence to each
-- side?

data Assoc = AssocL | AssocR | AssocNon
  deriving (Eq, Show)

data Prec = Prec
  { precBinding :: Int
  , precAssoc :: Assoc
  }
  deriving (Eq, Show)

makeFields ''Prec

precLe :: Prec -> Prec -> Maybe Bool
precLe (Prec b a) (Prec b' a')
  | b < b' = Just True
  | b > b' = Just False
  | otherwise = case (a, a') of
      (AssocL, AssocL) -> Just False
      (AssocR, AssocR) -> Just True
      _ -> Nothing

precs :: ConfTable Prec
precs =
  fromList
    [ (":", Prec 10 AssocNon)
    , ("->", Prec 20 AssocR)
    , ("=>", Prec 20 AssocR)
    , ("=", Prec 30 AssocNon)
    , ("+", Prec 50 AssocL)
    , ("-", Prec 50 AssocL)
    , ("*", Prec 60 AssocL)
    , ("/", Prec 60 AssocL)
    ]

-- The geolog grammar
--------------------------------------------------------------------------------

argStarts :: V.Vector T.Kind
argStarts = V.fromList [T.LParen, T.LBrack, T.AIdent, T.AKeyword, T.Field, T.Int, T.Block]

argStart :: T.Kind -> Bool
argStart k = V.elem k argStarts

tupleElems :: Parser [Ntn]
tupleElems =
  cur >>= \case
    T.RBrack -> pure []
    k | argStart k -> do
      n <- expr
      cur >>= \case
        T.RBrack -> pure [n]
        T.Comma -> advance >> (n :) <$> tupleElems
        k' -> do
          reportUnexpected k' T.CTupleMark
          pure [n]
    k -> do
      reportUnexpected k T.CExprStart
      pure []

arg :: Parser Ntn
arg = do
  m <- openingPos
  cur >>= \case
    T.LParen -> do
      advance
      skipState <- startSkipNewlines
      e <- expr
      expect T.RParen
      endSkipNewlines skipState
      pure e
    T.LBrack -> do
      advance
      skipState <- startSkipNewlines
      ns <- tupleElems
      expect T.RBrack
      endSkipNewlines skipState
      close m $ Tuple ns
    T.AIdent -> do
      x <- curQName
      advanceClose m $ Ident x
    T.AKeyword -> do
      x <- curName
      advanceClose m $ Keyword x
    T.Field -> do
      x <- curQName
      advanceClose m $ Field x
    T.Int -> do
      i <- curInt
      advanceClose m $ Int i
    T.Block -> block
    k -> do
      reportUnexpected k T.CExprStart
      s <- curSpan
      pure $ Error s

expr :: Parser Ntn
expr = arg >>= go (Prec 0 AssocNon)
 where
  go p lhs = do
    cur >>= \case
      k | k == T.SIdent || k == T.SKeyword -> do
        s <- curSpan
        (n, x) <-
          if k == T.SIdent
            then do
              qx@(QName _ x) <- curQName
              pure (Ident qx s, x)
            else do
              x <- curName
              pure (Keyword x s, x)
        p' <- case lookup precs x of
          Just p' -> pure p'
          Nothing -> do
            report s DefaultedPrec $
              "Defaulted precedence of" <+> pretty x <+> "to the same as +"
            pure $ Prec 50 AssocL
        case precLe p p' of
          Nothing -> do
            report s IncompatiblePrecedences "Incompatible precedences"
            pure lhs
          Just False -> pure lhs
          Just True -> do
            advance
            rhs <- arg >>= go p'
            pure $ Infix lhs n rhs
      k | argStart k -> do
        a <- arg
        go p (App lhs a)
      _ -> pure lhs

stmt :: Parser Ntn
stmt = do
  cur >>= \case
    T.Decl -> do
      m <- openingPos
      x <- curName
      advance
      n <- expr
      expect T.Nl
      pure $ Decl x n (Span m (endPos n))
    _ -> expr

stmts :: Parser [Ntn]
stmts = go []
 where
  go ns =
    cur >>= \case
      T.Nl -> do
        advance
        go ns
      k | k == T.End || k == T.Eof -> pure $ reverse ns
      _ -> do
        n <- stmt
        go $ n : ns

block :: Parser Ntn
block =
  cur >>= \case
    T.Block -> do
      m <- openingPos
      x <- curName
      advance
      h <-
        cur >>= \case
          k | argStart k -> Just <$> arg
          _ -> pure Nothing
      ns <- stmts
      advanceClose m $ Block x h ns
    _ -> error "expected a block"

-- Toplevel parsing interface
--------------------------------------------------------------------------------

parse :: Reporter -> File -> V.Vector T.Token -> IO [Ntn]
parse r f ts = do
  let s = State 0 256 False
  let e = Env ts f r
  evalStateT (runReaderT stmts.runParser e) s
