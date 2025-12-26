module Geolog.Lexer where

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

slice :: Lex T.Text
slice = do
  st <- get
  e <- ask
  pure $ TU.dropWord8 (st ^. prev) $ TU.takeWord8 (st ^. pos) (e ^. source)

slice1 :: Lex T.Text
slice1 = do
  st <- get
  e <- ask
  pure $ TU.dropWord8 (st ^. prev + 1) $ TU.takeWord8 (st ^. pos) (e ^. source)

isAlphaNum :: Char -> Bool
isAlphaNum c
  | isLetter c = True
  | isDigit c = True
  | c == '_' = True
  | otherwise = False

alphaNum1 :: Lex Name
alphaNum1 = do
  advance
  advanceWhile isAlphaNum
  s <- slice1
  pure $ Name $ Symbolize.intern s

alphaNum :: Lex Name
alphaNum = do
  advanceWhile isAlphaNum
  s <- slice
  pure $ Name $ Symbolize.intern s

int :: Lex Int
int = go 0
  where
    go i = do
      c <- peek
      if isDigit c
        then advance >> go (i * 10 + (ord c - ord '0'))
        else pure i

isLatinLetter :: Char -> Bool
isLatinLetter b
  | 'a' <= b && b <= 'z' = True
  | 'A' <= b && b <= 'Z' = True
  | otherwise = False

specialTable :: ConfTable Kind
specialTable =
  fromList
    [ ("theory", Block),
      ("instance", Block),
      ("def", Decl),
      ("let", Decl),
      ("open", Decl),
      ("import", Decl),
      ("end", End),
      ("=", SKeyword),
      (":", SKeyword),
      ("->", SKeyword)
    ]

fromName :: Kind -> Name -> Kind
fromName def x = case lookup specialTable x of
  Nothing -> def
  Just k -> k

emitName :: Kind -> Name -> Lex ()
emitName def x = emit (fromName def x) (VName x)

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
  advanceWhile isSymbol
  s <- slice
  pure $ Name $ Symbolize.intern s

error :: Char -> Lex ()
error c = do
  advance
  s <- span
  e <- ask
  let d = Diagnostic (Code.UnexpectedCharacter c) [Note (Just (SourceLoc (e ^. file) s)) Nothing]
  liftIO $ report (e ^. reporter) d
  emit0 Error

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
    '.' -> (alphaNum1 >>= emit Field . VName) >> toks
    '\'' -> (alphaNum1 >>= emit Tag . VName) >> toks
    '\0' -> emit0 Eof
    c | isDigit c -> (int >>= emit Int . VInt) >> toks
    c | isLetter c || c == '_' -> (alphaNum >>= emitName AIdent) >> toks
    c | isSymbol c -> (symbol >>= emitName SIdent) >> toks
    c -> error c >> toks

lex :: Reporter -> File -> IO (V.Vector Token)
lex r f = do
  let src = f ^. contents
  ts <- bufferWithCapacity (TU.lengthWord8 src)
  let s = State 0 0 (TU.iter src 0) ts
  let e = Env f r
  s' <- execStateT (runReaderT (runLex toks) e) s
  bufferUnsafeFreeze $ s' ^. tokens
