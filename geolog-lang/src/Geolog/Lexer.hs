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
  { statePos :: Int
  , statePrev :: Int
  , stateIter :: TU.Iter
  , stateTokens :: Buffer V.MVector Token
  }

makeFields ''State

data Env = Env
  { envFile :: File
  , envReporter :: Reporter
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

alphaNum :: Lex Name
alphaNum = do
  s <- use pos
  advanceWhile isAlphaNum
  e <- use pos
  x <- slice (Span s e)
  pure $ Name $ Symbolize.intern x

-- Parsing an identifier:
--
-- - Parse an alphanumeric name
-- - Check if the name is special (block, keyword, etc.), if so, return.
--   Special names don't participate in namespacing
-- - extend: If the next character is '/', keep going; look for either a symbol or
--   another alphanumeric segment.
-- - If the next one is a symbol, then check if the symbol is a "symbolic keyword";
--   if so, report an error, otherwise emit an `SIdent` token.
-- - If the next one is an alphanumeric name, then go back to `extend`
--
-- Note:
-- We should allow symbolic fields. E.g. .+ is a perfectly valid field.

-- Note:
-- Division can use unicode division (gah, why won't ormolu parse unicode??);
-- otherwise `a//` is too confusing. How often do you need to divide in a
-- database?

-- Note:
-- Do we want kebab case? It's very aesthetic, but perhaps is_type is just as
-- good as is-type? Subtraction is more common than division. Also, negative
-- numbers.  I think that if we want kebab case, we should require all binary
-- operators to have spaces around them, not just `-`. Which is not the end of
-- the world; this is maybe the right way to do it, and this is how it's done in
-- Pyret and Agda.
--
-- Then symbolic identifiers and alphanumeric identifiers may both contain `-`;
-- it's a wild card! When it starts a name, it makes that name symbolic, but it
-- can appear in either alphanumeric or symbolic names.

-- Note:
-- Should `Name` be a sumtype of symbolic vs. alphanumeric? If we keep it as is,
-- it's pretty easy to check by just looking at the first letter, and we can
-- always look up precedence in the table, so I think that keeping name as a
-- newtype wrapper around Symbol is going to be speediest.

-- Note:
-- Because we can always check if a name is a symbol or not, maybe we should do
-- away with different tokens for alphanums vs symbols? Nah, that's fine

-- Note:
-- We should call "symbol" something else, because Symbolize already uses the word
-- "symbol".

qname :: Lex ()
qname = do
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
    k' -> emit k' (VName x0)
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
            emit k (VQName (QName (reverse xs) x))
      _ -> emit k (VQName (QName (reverse xs) x))

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
    [ ("theory", Block)
    , ("instance", Block)
    , ("def", Decl)
    , ("let", Decl)
    , ("open", Decl)
    , ("import", Decl)
    , ("end", End)
    , ("=", SKeyword)
    , (":", SKeyword)
    , ("->", SKeyword)
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
  s <- use pos
  advanceWhile isSymbol
  e <- use pos
  x <- slice (Span s e)
  pure $ Name $ Symbolize.intern x

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
    '.' -> advance >> (alphaNum >>= emit Field . VName) >> toks
    '\'' -> advance >> (alphaNum >>= emit Tag . VName) >> toks
    '\0' -> emit0 Eof
    c | isDigit c -> (int >>= emit Int . VInt) >> toks
    c | isLetter c || c == '_' || isSymbol c -> qname >> toks
    c -> unexpectedChar c >> toks

lex :: Reporter -> File -> IO (V.Vector Token)
lex r f = do
  let src = f ^. contents
  ts <- bufferWithCapacity (TU.lengthWord8 src)
  let s = State 0 0 (TU.iter src 0) ts
  let e = Env f r
  s' <- execStateT (runReaderT (runLex toks) e) s
  bufferUnsafeFreeze $ s' ^. tokens
