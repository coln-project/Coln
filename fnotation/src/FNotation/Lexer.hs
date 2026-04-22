module FNotation.Lexer where

import Data.Char (isDigit, isLetter, ord)
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
import FNotation.Names
import FNotation.Tokens
import Prettyprinter
import Prelude hiding (error, getChar, head, init, last, lex, lookup, span, tail)
import Prelude qualified as P

-- Buffer
--------------------------------------------------------------------------------

data Buffer a = Buffer
  { next :: IORef Int,
    size :: Int,
    values :: VM.IOVector a
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
  deriving (Eq, Ord)

lexerCodeTable :: Map LexerCode CodeMeta
lexerCodeTable =
  Map.fromList
    [ (UnexpectedCharacter, CodeMeta 0 SError Nothing),
      (UncontinuedQualifiedName, CodeMeta 1 SError Nothing)
    ]

-- Lex monad
--------------------------------------------------------------------------------

data LexState = LexState
  { pos :: IORef Int,
    prev :: IORef Int,
    iter :: IORef TU.Iter,
    out :: Buffer Token,
    file :: File,
    reporter :: Reporter LexerCode,
    config :: ConfTable Kind
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

-- Lexemes
--------------------------------------------------------------------------------

sliceWhile :: LexState -> (Char -> Bool) -> IO Text
sliceWhile st f = do
  s <- readIORef st.pos
  advanceWhile st f
  e <- readIORef st.pos
  slice st (Span s e)

classifyName :: LexState -> Name -> Kind
classifyName st x = case confTableLookup st.config x.last of
  Just kind -> kind
  Nothing ->
    if isAlphaNumStart (T.head x.last)
      then AIdent
      else SIdent

isAlphaNumStart :: Char -> Bool
isAlphaNumStart c
  | isLetter c = True
  | c == '_' = True
  | otherwise = False

nameSeg :: LexState -> IO Text
nameSeg st =
  peek st >>= \case
    c | isAlphaNumStart c -> sliceWhile st isAlphaNum
    c | isSymbol c -> sliceWhile st isSymbol
    _ -> P.error "nameSeg should only be called if current character is a letter"

nameSegStart :: Char -> Bool
nameSegStart c = isAlphaNumStart c || isSymbol c

nameFromHeadTail :: Text -> [Text] -> Name
nameFromHeadTail head tail =
  let go s [] = ([], s)
      go s (t : ts) = let (ts', t') = go t ts in (s : ts', t')
      (init, last) = go head tail
   in Name init last

-- | Lex a name
name :: LexState -> IO Name
name st = nameFromHeadTail <$> nameSeg st <*> tail
  where
    tail =
      peek st >>= \case
        '/' -> do
          advance st
          peek st >>= \case
            c | nameSegStart c -> (:) <$> nameSeg st <*> tail
            _ -> do
              report st UncontinuedQualifiedName "expected continuation of qualified name"
              pure []
        _ -> pure []

ident :: LexState -> IO ()
ident st = do
  x <- name st
  emit st (classifyName st x) (VName x)

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
tryName st k d = peek st >>= \case
  c | nameSegStart c -> name st >>= emit st k . VName
  _ -> report st ExpectedName $ "expected a name after" <+> d

-- Top-level lexing interface
--------------------------------------------------------------------------------

-- | Lex all the tokens
run :: LexState -> IO ()
run st =
  peek st >>= \case
    '\t' -> skip st >> run st
    ' ' -> skip st >> run st
    '(' -> classify st LParen >> run st
    ')' -> classify st RParen >> run st
    '[' -> classify st LBrack >> run st
    ']' -> classify st RBrack >> run st
    '{' -> classify st LCurly >> run st
    '}' -> classify st RCurly >> run st
    ',' -> classify st Comma >> run st
    ';' -> classify st Semicolon >> run st
    '\n' -> classify st Nl >> run st
    '#' -> comment st >> run st
    '\"' -> string st >> run st
    '.' -> advance st >> tryName st Field "period" >> run st
    '\'' -> advance st >> tryName st Tag "single quote" >> run st
    '\0' -> emit0 st Eof
    c | isDigit c -> (int st >>= emit st Int . VInt) >> run st
    c | isLetter c || c == '_' || isSymbol c -> ident st >> run st
    c -> unexpectedChar st c >> run st

-- | Run the lexer, return a vector of tokens ready for parsing.
lex :: ConfTable Kind -> Reporter LexerCode -> File -> IO (V.Vector Token)
lex config reporter file = do
  pos <- newIORef 0
  prev <- newIORef 0
  iter <- newIORef $ TU.iter file.contents 0
  out <- bufferWithCapacity (TU.lengthWord8 file.contents + 1)
  let st = LexState pos prev iter out file reporter config
  run st
  bufferUnsafeFreeze st.out
