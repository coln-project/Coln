module Coln.FLIR.Sexp where

import Coln.Common hiding (Name, Span, Pos)

import FlatParse.Basic hiding (string)
import Data.Char (isLetter)
import Data.Text.Encoding (decodeUtf8)
import Prettyprinter qualified as Pp

data Atom = Int Int
          | String Text
          | VarId Int
          | Hash Text
          | Name Text
          | Keyword Text
  deriving (Show)

data Sexp a = Group [Sexp a] a | Atom Atom a
  deriving (Show)

data SexpError = UnclosedParen Pos | UnclosedString Pos
  deriving (Show)

int :: Parser SexpError Int
int = anyAsciiDecimalInt

-- TODO: handle escapes
string :: Parser SexpError Text
string = do
  $(char '\"')
  contents <- byteStringOf $ skipMany $ skipSatisfy (/= '\"')
  $(char '\"')
  pure $ decodeUtf8 contents

-- TODO: replace isLetter with more precise predicate
word :: Parser SexpError Text
word = fmap decodeUtf8 $ byteStringOf $ skipSome $ skipSatisfy isLetter

varid :: Parser SexpError Int
varid = $(char '%') >> int

name :: Parser SexpError Text
name = $(char '\'') >> word

isBase64 :: Char -> Bool
isBase64 c =
  ('A' <= c && c <= 'Z') ||
  ('a' <= c && c <= 'z') ||
  ('0' <= c && c <= '9') ||
  c == '+' || c == '/'

hash :: Parser SexpError Text
hash = $(char '#') >> (fmap decodeUtf8 $ byteStringOf $ skipMany $ skipSatisfy isBase64)

atom :: Parser SexpError Atom
atom =
  (Int <$> int) <|>
  (String <$> string) <|>
  (VarId <$> varid) <|>
  (Hash <$> hash) <|>
  (Name <$> name) <|>
  (Keyword <$> word)

group :: Parser SexpError [Sexp Span]
group = do
  $(char '(')
  exprs <- many sexp
  $(char ')')
  pure exprs

whitespace :: Parser e ()
whitespace = skipMany $ skipSatisfy (\c -> c == ' ' || c == '\t' || c == '\n')
  
sexp :: Parser SexpError (Sexp Span)
sexp = do
  whitespace
  flip withSpan (\f s -> pure (f s)) $
    (Atom <$> atom) <|>
    (Group <$> group)

instance DPretty Atom where
  dpretty = \case
    Int i -> pretty i
    String s -> "\"" <> pretty s <> "\""
    VarId i -> "%" <> pretty i
    Hash h -> "#" <> pretty h
    Name x -> "'" <> pretty x
    Keyword x -> pretty x

instance DPretty (Sexp e) where
  dpretty = \case
    Atom a _ -> dpretty a
    Group exprs _ -> "(" <> Pp.group (Pp.vsep (dpretty <$> exprs)) <> ")"
