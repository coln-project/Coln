module FNotation.Parser where

import Data.IORef
import Data.Map (Map)
import Data.Map qualified as Map
import Data.Text (Text)
import Data.Vector qualified as V
import Diagnostician
import FNotation.Config
import FNotation.Names
import FNotation.Tokens qualified as T
import FNotation.Trees
import Prettyprinter
import Prelude hiding (lookup)

-- Parser diagnostics
--------------------------------------------------------------------------------

data ParserCode
  = UnexpectedToken
  | DefaultedPrec
  | IncompatiblePrecedences
  deriving (Eq, Ord)

parserCodeTable :: Map ParserCode CodeMeta
parserCodeTable =
  Map.fromList
    [ (UnexpectedToken, CodeMeta 0 SError Nothing),
      (DefaultedPrec, CodeMeta 1 SWarning Nothing),
      (IncompatiblePrecedences, CodeMeta 2 SError Nothing)
    ]

-- Parser monad
--------------------------------------------------------------------------------

data ParseState = ParseState
  { pos :: IORef Int,
    gas :: IORef Int,
    skipNewlines :: IORef Bool,
    tokens :: V.Vector T.Token,
    file :: File,
    reporter :: Reporter ParserCode,
    config :: ConfTable Prec
  }

-- Parsing utilities
--------------------------------------------------------------------------------

report :: ParseState -> Span -> ParserCode -> DDoc -> IO ()
report st s c m = do
  let n = Note (Just (SourceLoc st.file s)) Nothing
  let d = Diagnostic c m [n]
  st.reporter.reportIO d

cur :: ParseState -> IO T.Kind
cur st = do
  gas <- readIORef st.gas
  if gas <= 0
    then do
      pos <- readIORef st.pos
      let token = st.tokens V.! pos
      error $ "out of gas at token " ++ show (dpretty token)
    else writeIORef st.gas (gas - 1)
  pos <- readIORef st.pos
  pure (st.tokens V.! pos).kind

locally :: IORef a -> a -> IO b -> IO b
locally ref v action = do
  old <- readIORef ref
  writeIORef ref v
  res <- action
  writeIORef ref old
  pure res

ignoreNewlines :: ParseState -> IO a -> IO a
ignoreNewlines st = locally st.skipNewlines True

withNewlines :: ParseState -> IO a -> IO a
withNewlines st = locally st.skipNewlines False

curSpan :: ParseState -> IO Span
curSpan st = do
  pos <- readIORef st.pos
  pure (V.unsafeIndex st.tokens pos).span

curValue :: ParseState -> IO T.TokenValue
curValue st = do
  pos <- readIORef st.pos
  pure (V.unsafeIndex st.tokens pos).value

curName :: ParseState -> IO Name
curName st =
  curValue st >>= \case
    T.VName x -> pure x
    _ -> error "expected token to be associated with a name"

curInt :: ParseState -> IO Int
curInt st =
  curValue st >>= \case
    T.VInt x -> pure x
    _ -> error "expected token to be associated with an int"

curString :: ParseState -> IO Text
curString st =
  curValue st >>= \case
    T.VString x -> pure x
    _ -> error "expected token to be associated with a string"

at :: ParseState -> T.Kind -> IO Bool
at st k = (k ==) <$> cur st

advance :: ParseState -> IO ()
advance st = do
  pos <- readIORef st.pos
  let n = V.length st.tokens
  if pos < n - 1
    then do
      let next j
            | j < n = case (V.unsafeIndex st.tokens j).kind of
                T.Nl -> next (j + 1)
                _ -> j
            | otherwise = j
      readIORef st.skipNewlines >>= \case
        True -> writeIORef st.pos $ next (pos + 1)
        False -> writeIORef st.pos $ pos + 1
      writeIORef st.gas 256
    else pure ()

eat :: ParseState -> T.Kind -> IO Bool
eat st k =
  at st k >>= \case
    True -> advance st >> pure True
    False -> pure False

reportUnexpected :: ParseState -> T.Kind -> T.Class -> IO ()
reportUnexpected st k c = do
  s <- curSpan st
  report st s UnexpectedToken $
    "Unexpected token kind" <+> dpretty k <> ", expected" <+> dpretty c

expect :: ParseState -> T.Kind -> IO ()
expect st k = do
  k' <- cur st
  if k == k'
    then advance st
    else
      reportUnexpected st k' (T.CSpecific k) >> pure ()

openingPos :: ParseState -> IO Pos
openingPos st = (.start) <$> curSpan st

close :: ParseState -> Pos -> (Span -> Ntn) -> IO Ntn
close st s f = do
  (Span _ e) <- curSpan st
  pure $ f (Span s e)

advanceClose :: ParseState -> Pos -> (Span -> Ntn) -> IO Ntn
advanceClose st s f = do
  n <- close st s f
  advance st
  pure n

-- The fnotation grammar
--------------------------------------------------------------------------------

argStarts :: V.Vector T.Kind
argStarts =
  V.fromList
    [ T.LParen,
      T.LBrack,
      T.AIdent,
      T.AKeyword,
      T.Field,
      T.Tag,
      T.Int,
      T.Block
    ]

argStart :: T.Kind -> Bool
argStart k = V.elem k argStarts

tupleElems :: ParseState -> IO [Ntn]
tupleElems st =
  cur st >>= \case
    T.RBrack -> pure []
    k | argStart k -> do
      n <- expr st
      cur st >>= \case
        T.RBrack -> pure [n]
        T.Comma -> do
          advance st
          ns <- tupleElems st
          pure $ n : ns
        k' -> do
          reportUnexpected st k' T.CTupleMark
          pure [n]
    k -> do
      reportUnexpected st k T.CExprStart
      pure []

arg :: ParseState -> IO Ntn
arg st = do
  m <- openingPos st
  cur st >>= \case
    T.LParen -> do
      e <- ignoreNewlines st $ do
        advance st
        expr st
      expect st T.RParen
      pure e
    T.LBrack -> do
      ns <- ignoreNewlines st $ do
        advance st
        tupleElems st
      expect st T.RBrack
      close st m $ Tuple ns
    T.AIdent -> do
      x <- curName st
      advanceClose st m $ Ident x
    T.AKeyword -> do
      x <- curName st
      advanceClose st m $ Keyword x
    T.Field -> do
      x <- curName st
      advanceClose st m $ Field x
    T.Tag -> do
      x <- curName st
      advanceClose st m $ Tag x
    T.Int -> do
      i <- curInt st
      advanceClose st m $ Int i
    T.String -> do
      x <- curString st
      advanceClose st m $ String x
    T.Block -> block st
    k -> do
      reportUnexpected st k T.CExprStart
      advanceClose st m Error

args :: ParseState -> IO [Ntn]
args st = do
  k <- cur st
  if argStart k
    then (:) <$> arg st <*> args st
    else pure []

expr :: ParseState -> IO Ntn
expr st = arg st >>= go (Prec 0 AssocNon)
  where
    go p lhs = do
      cur st >>= \case
        k@(T.SIdent; T.SKeyword) -> do
          s <- curSpan st
          x <- curName st
          let n = case k of
                T.SIdent -> Ident x s
                T.SKeyword -> Keyword x s
          p' <- case confTableLookup st.config x.last of
            Just p' -> pure p'
            Nothing -> do
              report st s DefaultedPrec $
                "Defaulted precedence of" <+> dpretty x <+> "to the same as +"
              pure $ Prec 50 AssocL
          case precLe p p' of
            Nothing -> do
              report st s IncompatiblePrecedences "Incompatible precedences"
              pure lhs
            Just False -> pure lhs
            Just True -> do
              advance st
              rhs <- arg st >>= go p'
              go p (Infix lhs n rhs)
        k | argStart k -> do
          spine <- args st
          go p (App lhs spine)
        _ -> pure lhs

stmt :: ParseState -> IO Ntn
stmt st = do
  cur st >>= \case
    T.Decl -> do
      m <- openingPos st
      x <- curName st
      advance st
      n <- expr st
      expect st T.Nl
      pure $ Decl x n (Span m (endPos n))
    _ -> expr st

stmts :: ParseState -> IO [Ntn]
stmts st = go []
  where
    go ns =
      cur st >>= \case
        T.Nl -> do
          advance st
          go ns
        k | k == T.End || k == T.Eof -> pure $ reverse ns
        _ -> do
          n <- stmt st
          go $ n : ns

block :: ParseState -> IO Ntn
block st =
  cur st >>= \case
    T.Block -> do
      m <- openingPos st
      x <- curName st
      advance st
      h <-
        cur st >>= \case
          k | argStart k -> Just <$> arg st
          _ -> pure Nothing
      ns <- stmts st
      advanceClose st m $ Block x h ns
    _ -> error "expected a block"

-- Toplevel parsing interface
--------------------------------------------------------------------------------

parse :: ConfTable Prec -> Reporter ParserCode -> File -> V.Vector T.Token -> IO [Ntn]
parse config reporter file tokens = do
  pos <- newIORef 0
  gas <- newIORef 256
  skipNewlines <- newIORef False
  let st = ParseState pos gas skipNewlines tokens file reporter config
  stmts st
