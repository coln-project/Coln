-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module FNotation.Lexer where

import Data.Char (isDigit, isLetter, isPrint, ord)
import Data.IORef
import Data.Map (Map)
import Data.Map qualified as Map
import Data.Text (Text)
import Data.Text qualified as T
import Data.Text.Unsafe qualified as TU
import Data.Vector qualified as V
import Data.Vector.Mutable qualified as VM
import Diagnostician
import FNotation.Config
import FNotation.Kinds
import FNotation.Names
import FNotation.Tokens
import Prettyprinter
import Prelude hiding (error, getChar, head, init, last, lex, lookup, span, tail)
import Prelude qualified as P

-- Buffer
--------------------------------------------------------------------------------

data Buffer a = Buffer
  { next :: IORef Int
  , size :: Int
  , values :: VM.IOVector a
  }

bufferWithCapacity :: Int -> IO (Buffer a)
bufferWithCapacity n = do
  next <- newIORef 0
  values <- VM.unsafeNew n
  pure $ Buffer next n values

bufferUnsafeFreeze :: Buffer a -> IO (V.Vector a)
bufferUnsafeFreeze b = do
  l <- readIORef b.next
  V.take l <$> V.unsafeFreeze b.values

push :: Buffer a -> a -> IO ()
push b e = do
  i <- readIORef b.next
  if (i < b.size)
    then do
      VM.unsafeWrite b.values i e
      writeIORef b.next (i + 1)
    else P.error "cannot push because buffer is full"

-- Lexer diagnostics
--------------------------------------------------------------------------------

data LexerCode
  = UnexpectedCharacter
  | UncontinuedQualifiedName
  | ExpectedName
  | InvalidNamespace
  | KeywordInNamespace
  | InvalidKeyword
  | EmptyNameComponent
  deriving (Eq, Ord)

lexerCodeTable :: Map LexerCode CodeMeta
lexerCodeTable =
  Map.fromList
    [ (UnexpectedCharacter, CodeMeta 0 SError Nothing)
    , (UncontinuedQualifiedName, CodeMeta 1 SError Nothing)
    , (ExpectedName, CodeMeta 2 SError Nothing)
    , (InvalidNamespace, CodeMeta 3 SError Nothing)
    , (KeywordInNamespace, CodeMeta 4 SError Nothing)
    , (InvalidKeyword, CodeMeta 5 SError Nothing)
    , (EmptyNameComponent, CodeMeta 6 SError Nothing)
    ]

-- Lex monad
--------------------------------------------------------------------------------

data LexState = LexState
  { pos :: IORef Int
  , prev :: IORef Int
  , iter :: IORef TU.Iter
  , out :: Buffer Token
  , file :: File
  , reporter :: Reporter LexerCode
  , config :: ConfTable Kind
  }

-- Fundamental lexing actions
--------------------------------------------------------------------------------

span :: LexState -> IO Span
span st = Span <$> readIORef st.prev <*> readIORef st.pos

emit :: LexState -> Kind -> TokenValue -> IO ()
emit st k v = do
  s <- span st
  push st.out (Token k v s)
  readIORef st.pos >>= writeIORef st.prev

emit0 :: LexState -> Kind -> IO ()
emit0 st k = emit st k VEmpty

-- peek the character being currently scrutinized
peek :: LexState -> IO Char
peek st = do
  TU.Iter c _ <- readIORef st.iter
  pure c

advance :: LexState -> IO ()
advance st = do
  let src = st.file.contents
  TU.Iter _ j <- readIORef st.iter
  i <- readIORef st.pos
  let i' = i + j
  writeIORef st.pos i'
  if i' >= TU.lengthWord8 src
    then writeIORef st.iter (TU.Iter '\0' 0)
    else writeIORef st.iter (TU.iter src i')

classify :: LexState -> Kind -> IO ()
classify st k = advance st >> emit0 st k

skip :: LexState -> IO ()
skip st = do
  advance st
  readIORef st.pos >>= writeIORef st.prev

advanceWhile :: LexState -> (Char -> Bool) -> IO ()
advanceWhile st f =
  peek st >>= \case
    '\0' -> pure ()
    c ->
      if f c
        then advance st >> advanceWhile st f
        else pure ()

slice :: LexState -> Span -> IO T.Text
slice st s = pure $ sliceWord8 s.start s.end st.file.contents

isAlphaNum :: Char -> Bool
isAlphaNum c
  | isLetter c = True
  | isDigit c = True
  | c == '_' || c == '-' = True
  | otherwise = False

-- TODO: more symbols, including unicode symbols? See issue #6
isSymbol :: Char -> Bool
isSymbol = \case
  '<' -> True
  '>' -> True
  '-' -> True
  '+' -> True
  '/' -> True
  '*' -> True
  '~' -> True
  ':' -> True
  '=' -> True
  '@' -> True
  _ -> False

report :: LexState -> LexerCode -> DDoc -> IO ()
report st c m = do
  s <- span st
  let d = Diagnostic c m [Note (Just (SourceLoc (st.file) s)) Nothing]
  st.reporter.reportIO d

unexpectedChar :: LexState -> Char -> IO ()
unexpectedChar st c = do
  advance st
  report st UnexpectedCharacter $ "Unexpected character" <+> "'" <> pretty c <> "'"

emptyNameComponent :: LexState -> IO ()
emptyNameComponent st = do
  report st EmptyNameComponent $ "Empty name component"

-- Lexemes
--------------------------------------------------------------------------------

sliceWhile :: LexState -> (Char -> Bool) -> IO Text
sliceWhile st f = do
  s <- readIORef st.pos
  advanceWhile st f
  e <- readIORef st.pos
  slice st (Span s e)

classifyNameSeg :: LexState -> Text -> Kind
classifyNameSeg st x = case confTableLookup st.config x of
  Just kind -> kind
  Nothing ->
    if isAlphaNumStart (T.head x)
      then AIdent
      else SIdent

isAlphaNumStart :: Char -> Bool
isAlphaNumStart c
  | isLetter c = True
  | c == '_' = True
  | otherwise = False

nameSeg :: LexState -> IO (Kind, Text)
nameSeg st =
  peek st >>= \case
    c | isAlphaNumStart c -> do
      t <- sliceWhile st isAlphaNum
      let k = classifyNameSeg st t
      pure (k, t)
    c | isSymbol c -> do
      t <- sliceWhile st isSymbol
      let k = classifyNameSeg st t
      pure (k, t)
    c | c == '`' -> do
      advance st
      t <- sliceWhile st (\c' -> c' /= '`' && isPrint c')
      c' <- peek st
      if c' == '`'
        then advance st
        else unexpectedChar st c'
      if T.null t
        then emptyNameComponent st
        else pure ()
      pure (AIdent, t)
    _ -> P.error "nameSeg should only be called if current character is a letter, symbol, or backtick"

nameSegStart :: Char -> Bool
nameSegStart c = isAlphaNumStart c || isSymbol c || c == '`'

-- | Lex a name
name :: LexState -> Maybe Kind -> IO ()
name st wanted = do
  (k, t) <- nameSeg st
  go k t []
 where
  go k t stack =
    peek st >>= \case
      '/' -> do
        case k of
          AIdent -> pure ()
          _ -> report st InvalidNamespace "used keyword as namespace component"
        advance st
        peek st >>= \case
          c | nameSegStart c -> do
            (k', t') <- nameSeg st
            go k' t' (t : stack)
          _ -> do
            report st UncontinuedQualifiedName "expected continuation of qualified name"
            emitName k t stack
      _ -> emitName k t stack
  emitName k t stack = case wanted of
    Nothing -> case k of
      AIdent; SIdent -> emit st k (VName $ Name (reverse stack) t)
      _ -> case stack of
        [] -> emit st k (VName $ Name (reverse stack) t)
        _ -> do
          report st KeywordInNamespace "referred to keyword as a qualified name"
          let k' = if T.all isSymbol $ T.take 1 t then SIdent else AIdent
          emit st k' (VName $ Name (reverse stack) t)
    Just k' -> case k of
      AIdent -> emit st k' (VName $ Name (reverse stack) t)
      _ -> do
        report st InvalidKeyword $ "used reserved word as a" <+> dpretty k'
        emit st k' (VName $ Name (reverse stack) t)

ident :: LexState -> IO ()
ident st = name st Nothing

int :: LexState -> IO Int
int st = go 0
 where
  go i = do
    c <- peek st
    if isDigit c
      then advance st >> go (i * 10 + (ord c - ord '0'))
      else pure i

comment :: LexState -> IO ()
comment st = do
  advanceWhile st (/= '\n')
  advance st

string :: LexState -> IO ()
string st = do
  s <- readIORef st.pos
  advance st
  advanceWhile st (/= '\"')
  advance st
  e <- readIORef st.pos
  x <- slice st (Span (s + 1) (e - 1))
  emit st String (VString x)

tryName :: LexState -> Kind -> DDoc -> IO ()
tryName st k d =
  peek st >>= \case
    c | nameSegStart c -> name st (Just k)
    _ -> report st ExpectedName $ "expected a name after" <+> d

-- Top-level lexing interface
--------------------------------------------------------------------------------

-- | Lex all the tokens
run :: LexState -> Bool -> IO ()
run st ws =
  peek st >>= \case
    '\t' -> skip st >> run st True
    ' ' -> skip st >> run st True
    '(' -> classify st LParen >> run st False
    ')' -> classify st RParen >> run st False
    '[' -> classify st LBrack >> run st False
    ']' -> classify st RBrack >> run st False
    '{' -> classify st LCurly >> run st False
    '}' -> classify st RCurly >> run st False
    ',' -> classify st Comma >> run st False
    ';' -> classify st Semicolon >> run st False
    '\n' -> classify st Nl >> run st False
    '#' -> comment st >> run st False
    '\"' -> string st >> run st False
    '.' -> advance st >> tryName st (if ws then Field else FieldImmediate) "period" >> run st False
    '\'' -> advance st >> tryName st Tag "single quote" >> run st False
    '\0' -> emit0 st Eof
    '`' -> ident st >> run st False
    c | isDigit c -> (int st >>= emit st Int . VInt) >> run st False
    c | isLetter c || c == '_' || isSymbol c -> ident st >> run st False
    c -> unexpectedChar st c >> run st False

-- | Run the lexer, return a vector of tokens ready for parsing.
lex :: ConfTable Kind -> Reporter LexerCode -> File -> IO (V.Vector Token)
lex config reporter file = do
  pos <- newIORef 0
  prev <- newIORef 0
  iter <- newIORef $ TU.iter file.contents 0
  out <- bufferWithCapacity (TU.lengthWord8 file.contents + 1)
  let st = LexState pos prev iter out file reporter config
  run st False
  bufferUnsafeFreeze st.out
