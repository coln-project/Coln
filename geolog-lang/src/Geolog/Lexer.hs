module Geolog.Lexer (lex) where

import Control.Monad.IO.Class
import Control.Monad.Reader (ReaderT, runReaderT)
import Control.Monad.Reader.Class
import Control.Monad.State.Class
import Control.Monad.State.Strict (StateT, execStateT)
import Data.Char (isDigit, isLetter, ord)
import Data.Text qualified as T
import Data.Text.Unsafe qualified as TU
import Data.Vector qualified as V
import Geolog.Common
import Geolog.Diagnostics
import Geolog.Diagnostics.Code qualified as Code
import Geolog.Token
import Lens.Micro.Platform
import Symbolize qualified
import Prelude hiding (error, getChar, lex, lookup, span)

-- Lex monad
--------------------------------------------------------------------------------

data State = State
  { statePos :: Int,
    statePrev :: Int,
    stateIter :: TU.Iter,
    stateTokens :: Buffer V.MVector Token
  }

makeFields ''State

data Env = Env
  { envFile :: File,
    envReporter :: Reporter
  }

makeFields ''Env

source :: Lens' Env T.Text
source = file . contents

newtype Lex a = Lex {runLex :: ReaderT Env (StateT State IO) a}
  deriving (Functor, Applicative, Monad, MonadIO, MonadState State, MonadReader Env)

-- Fundamental lexing actions
--------------------------------------------------------------------------------

emit0 :: Kind -> Lex ()
emit0 k = emit k VEmpty

span :: Lex Span
span = do
  st <- get
  pure $ Span (st ^. prev) (st ^. pos)

emit :: Kind -> TokenValue -> Lex ()
emit k v = do
  s <- span
  push tokens (Token k v s)
  prev <~ use pos

-- peek a byte as a character
peek :: Lex Char
peek = do
  TU.Iter c _ <- use iter
  pure c

advance :: Lex ()
advance = do
  src <- view source
  TU.Iter _ j <- use iter
  i <- use pos
  let i' = i + j
  pos .= i'
  if i' >= TU.lengthWord8 src
    then iter .= TU.Iter '\0' 0
    else iter .= TU.iter src i'

classify :: Kind -> Lex ()
classify k = advance >> emit0 k

skip :: Lex ()
skip = do
  advance
  prev <~ use pos

advanceWhile :: (Char -> Bool) -> Lex ()
advanceWhile f =
  peek >>= \case
    '\0' -> pure ()
    c ->
      if f c
        then advance >> advanceWhile f
        else pure ()

slice :: Span -> Lex T.Text
slice (Span s e) = do
  env <- ask
  pure $ sliceWord8 s e (env ^. source)

isAlphaNum :: Char -> Bool
isAlphaNum c
  | isLetter c = True
  | isDigit c = True
  | c == '_' || c == '-' = True
  | otherwise = False

report :: Code.Code -> Lex ()
report c = do
  s <- span
  e <- ask
  let d = Diagnostic c [Note (Just (SourceLoc (e ^. file) s)) Nothing]
  liftIO $ reportIO (e ^. reporter) d

unexpectedChar :: Char -> Lex ()
unexpectedChar c = do
  advance
  report (Code.UnexpectedCharacter c)

-- Lexemes
--------------------------------------------------------------------------------

alphaNum :: Lex Name
alphaNum = do
  s <- use pos
  advanceWhile isAlphaNum
  e <- use pos
  x <- slice (Span s e)
  pure $ Name $ Symbolize.intern x

specialTable :: ConfTable Kind
specialTable =
  fromList
    [ ("sig", Block),
      ("theory", Decl),
      ("def", Decl),
      ("let", Decl),
      ("open", Decl),
      ("import", Decl),
      ("end", End),
      ("Query", AKeyword),
      ("=", SKeyword),
      (":=", SKeyword),
      (":", SKeyword),
      ("->", SKeyword)
    ]

-- | Lex a qualified name, and return its kind (configured in @specialTable@)
qname' :: Lex (QName, Kind)
qname' = do
  (x0, k) <-
    peek >>= \case
      c | isLetter c || c == '_' -> do
        x0 <- alphaNum
        pure (x0, AIdent)
      c | isSymbol c -> do
        x0 <- symbol
        pure (x0, SIdent)
      _ -> impossible
  case fromName k x0 of
    k' | k == k' -> go [] x0 k
    k' -> pure (QName [] x0, k')
  where
    go xs x k =
      peek >>= \case
        '/' -> do
          advance
          peek >>= \case
            c | isLetter c -> do
              x' <- alphaNum
              go (x : xs) x' AIdent
            c | isSymbol c -> do
              x' <- symbol
              go (x : xs) x' SIdent
            _ -> do
              report Code.UncontinuedQualifiedName
              pure (QName (reverse xs) x, k)
        _ -> pure (QName (reverse xs) x, k)

-- | Lex a qualified name and return it, ignoring kind
qname :: Lex QName
qname = fst <$> qname'

ident :: Lex ()
ident = do
  (x, k) <- qname'
  if k == SIdent || k == AIdent
    then emit k (VQName x)
    else emit k (VName $ qnameBase x)

int :: Lex Int
int = go 0
  where
    go i = do
      c <- peek
      if isDigit c
        then advance >> go (i * 10 + (ord c - ord '0'))
        else pure i

fromName :: Kind -> Name -> Kind
fromName def x = case lookup specialTable x of
  Nothing -> def
  Just k -> k

emitName :: Kind -> Name -> Lex ()
emitName def x = emit (fromName def x) (VName x)

-- TODO: more symbols, including unicode symbols? See issue #6
isSymbol :: Char -> Bool
isSymbol = \case
  '<' -> True
  '>' -> True
  '-' -> True
  '+' -> True
  '/' -> True
  '*' -> True
  ':' -> True
  '=' -> True
  _ -> False

symbol :: Lex Name
symbol = do
  s <- use pos
  advanceWhile isSymbol
  e <- use pos
  x <- slice (Span s e)
  pure $ Name $ Symbolize.intern x

-- Top-level lexing interface
--------------------------------------------------------------------------------

-- | Lex all the tokens
--
-- This has a short name so that the tail-recursion is less annoying to write
-- out.
toks :: Lex ()
toks =
  peek >>= \case
    '\t' -> skip >> toks
    ' ' -> skip >> toks
    '(' -> classify LParen >> toks
    ')' -> classify RParen >> toks
    '[' -> classify LBrack >> toks
    ']' -> classify RBrack >> toks
    '{' -> classify LCurly >> toks
    '}' -> classify RCurly >> toks
    ',' -> classify Comma >> toks
    ';' -> classify Semicolon >> toks
    '\n' -> classify Nl >> toks
    '.' -> advance >> (qname >>= emit Field . VQName) >> toks
    '\'' -> advance >> (qname >>= emit Tag . VQName) >> toks
    '\0' -> emit0 Eof
    c | isDigit c -> (int >>= emit Int . VInt) >> toks
    c | isLetter c || c == '_' || isSymbol c -> ident >> toks
    c -> unexpectedChar c >> toks

-- | The only exported function; run the lexer, return a vector of tokens ready
-- for parsing.
lex :: Reporter -> File -> IO (V.Vector Token)
lex r f = do
  let src = f ^. contents
  ts <- bufferWithCapacity (TU.lengthWord8 src)
  let s = State 0 0 (TU.iter src 0) ts
  let e = Env f r
  s' <- execStateT (runReaderT (runLex toks) e) s
  bufferUnsafeFreeze $ s' ^. tokens
