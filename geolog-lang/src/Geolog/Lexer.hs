module Geolog.Lexer where

import Control.Monad.IO.Class
import Control.Monad.Reader (ReaderT, runReaderT)
import Control.Monad.Reader.Class
import Control.Monad.State.Class
import Control.Monad.State.Strict (StateT, execStateT)
import Data.Bits
import Data.ByteString qualified as BS
import Data.ByteString.Internal (w2c)
import Data.Char (chr, isDigit, isLetter, ord)
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
  , stateTokens :: Buffer V.MVector Token
  }

makeFields ''State

data Env = Env
  { envFile :: File
  , envReporter :: Reporter
  }

makeFields ''Env

source :: Lens' Env BS.ByteString
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
peekb :: Lex (Maybe Char)
peekb = do
  st <- get
  e <- ask
  pure $ w2c <$> BS.indexMaybe (e ^. source) (st ^. pos)

advance :: Int -> Lex ()
advance j = pos += j

advanceb :: Lex ()
advanceb = advance 1

classifyb :: Kind -> Lex ()
classifyb k = advanceb >> emit0 k

skipb :: Lex ()
skipb = do
  pos += 1
  prev <~ use pos

getByte :: BS.ByteString -> Int -> Int
getByte bs i = fromIntegral $ BS.index bs i

getChar :: BS.ByteString -> Int -> (Char, Int)
getChar bs i = over _1 chr $ case b 0 of
  c | c <= 0x7F -> (c, 1)
  c
    | c <= 0xDF ->
        ( ((c - 0xC0) `shift` 6)
            .|. (b 1 - 0x80)
        , 2
        )
  c
    | c <= 0xEF ->
        ( ((c - 0xE0) `shift` 12)
            .|. ((b 1 - 0x80) `shift` 6)
            .|. (b 2 - 0x80)
        , 3
        )
  c ->
    ( ((c - 0xF0) `shift` 18)
        .|. ((b 1 - 0x80) `shift` 12)
        .|. ((b 2 - 0x80) `shift` 6)
        .|. (b 3 - 0x80)
    , 4
    )
 where
  b j = getByte bs (i + j)

peekc :: Lex (Char, Int)
peekc = do
  st <- get
  e <- ask
  pure $ getChar (e ^. source) (st ^. pos)

advancebWhile :: (Char -> Bool) -> Lex ()
advancebWhile f =
  peekb >>= \case
    Just b ->
      if f b
        then pos += 1 >> advancebWhile f
        else pure ()
    Nothing -> pure ()

advanceWhile :: (Char -> Bool) -> Lex ()
advanceWhile f = do
  (c, j) <- peekc
  if f c
    then pos += j >> advanceWhile f
    else pure ()

slice :: Lex BS.ByteString
slice = do
  st <- get
  e <- ask
  pure $ BS.drop (st ^. prev) $ BS.take (st ^. pos) (e ^. source)

-- Slice all but the first character, for field/tag
slice1 :: Lex BS.ByteString
slice1 = do
  st <- get
  e <- ask
  pure $ BS.take (st ^. pos) $ BS.drop (st ^. prev + 1) (e ^. source)

isAlphaNum :: Char -> Bool
isAlphaNum c
  | isLetter c = True
  | isDigit c = True
  | c == '_' = True
  | otherwise = False

alphaNum1 :: Lex Name
alphaNum1 = do
  advanceb
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
  go i =
    peekb >>= \case
      Just b ->
        if isDigit b
          then advanceb >> go (i * 10 + (ord b - ord '0'))
          else pure i
      Nothing -> pure i

isLatinLetter :: Char -> Bool
isLatinLetter b
  | 'a' <= b && b <= 'z' = True
  | 'A' <= b && b <= 'Z' = True
  | otherwise = False

specialTable :: ConfTable Kind
specialTable =
  fromList
    [ ("theory", Decl)
    , ("instance", Decl)
    , ("def", Decl)
    , ("let", Decl)
    , ("open", Decl)
    , ("import", Decl)
    , ("sig", Block)
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
  advancebWhile isSymbol
  s <- slice
  pure $ Name $ Symbolize.intern s

error :: Char -> Int -> Lex ()
error c i = do
  advance i
  s <- span
  e <- ask
  let d = Diagnostic (Code.UnexpectedCharacter c) [Note (Just (SourceLoc (e ^. file) s)) Nothing]
  liftIO $ report (e ^. reporter) d
  emit0 Error

toks :: Lex ()
toks =
  peekb >>= \case
    Just b ->
      case b of
        '\t' -> skipb >> toks
        ' ' -> skipb >> toks
        '(' -> classifyb LParen >> toks
        ')' -> classifyb RParen >> toks
        '[' -> classifyb LBrack >> toks
        ']' -> classifyb RBrack >> toks
        '{' -> classifyb LCurly >> toks
        '}' -> classifyb RCurly >> toks
        ',' -> classifyb Comma >> toks
        ';' -> classifyb Semicolon >> toks
        '\n' -> classifyb Nl >> toks
        '.' -> (alphaNum1 >>= emit Field . VName) >> toks
        '\'' -> (alphaNum1 >>= emit Tag . VName) >> toks
        _ | isDigit b -> (int >>= emit Int . VInt) >> toks
        _ | isLatinLetter b || b == '_' -> (alphaNum >>= emitName AIdent) >> toks
        _ | isSymbol b -> (symbol >>= emitName SIdent) >> toks
        _
          | b >= '\x80' ->
              peekc >>= \case
                (c, j) | isLetter c -> do
                  advance j
                  alphaNum >>= emitName AIdent
                  toks
                (c, j) -> error c j >> toks
        _ -> error b 1 >> toks
    Nothing -> emit0 Eof

lex :: Reporter -> File -> IO (V.Vector Token)
lex r f = do
  let src = f ^. contents
  ts <- bufferWithCapacity (BS.length src)
  let s = State 0 0 ts
  let e = Env f r
  s' <- execStateT (runReaderT (runLex toks) e) s
  bufferUnsafeFreeze $ s' ^. tokens
