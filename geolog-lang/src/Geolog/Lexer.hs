module Geolog.Lexer where

import Data.Char (isLetter)
import Data.Vector.Hashtables (FrozenDictionary)
import Data.Vector.Hashtables qualified as HT
import Data.Vector.Strict as V
import FlatParse.Basic hiding (Parser, runParser)
import Geolog.Common
import Geolog.Notation
import System.IO.Unsafe (unsafePerformIO)

type Parser = ParserIO ()

lineComment :: Parser ()
lineComment =
  withOption
    anyWord8
    ( \case
        10 -> ws
        _ -> lineComment
    )
    (pure ())

-- | Parse a potentially nested multiline comment.
multilineComment :: Parser ()
multilineComment = go (1 :: Int)
 where
  go 0 = ws
  go n =
    $( switch
         [|
           case _ of
             "-}" -> go (n - 1)
             "{-" -> go (n + 1)
             _ -> branch anyWord8 (go n) (pure ())
           |]
     )

ws :: Parser ()
ws =
  $( switch
       [|
         case _ of
           " " -> ws
           "\t" -> ws
           "\r" -> ws
           "--" -> lineComment
           "{-" -> multilineComment
           _ -> pure ()
         |]
   )

nameStartChar :: Parser ()
nameStartChar = fusedSatisfy isLatinLetter isLetter isLetter isLetter >> pure ()

isNameAsciiChar :: Char -> Bool
isNameAsciiChar c = isLatinLetter c || isDigit c || c == '-'

isNameChar :: Char -> Bool
isNameChar c = isLetter c || isDigit c || c == '-'

opChar :: Parser ()
opChar =
  $( switch
       [|
         case _ of
           "<" -> pure ()
           ">" -> pure ()
           "-" -> pure ()
           "+" -> pure ()
           "/" -> pure ()
           "*" -> pure ()
           ":" -> pure ()
           "=" -> pure ()
           _ -> failed
         |]
   )

data OpData = KwOp Prec | UserOp Prec

ops :: FrozenDictionary V.Vector RawName V.Vector OpData
ops = unsafePerformIO (HT.fromList l >>= HT.unsafeFreeze)
 where
  l = [(":", KwOp (NonAssoc 20))]

op :: Parser Token
op = do
  n <- RawName <$> byteStringOf (some opChar)
  case HT.findElem ops n of
    -1 -> error "no registered precedence"
    i -> pure $ case (HT.fvalue ops V.! i) of
      KwOp p -> KEYWORD_OP (NRawName n) p
      UserOp p -> OP (NRawName n) p

nameChar :: Parser ()
nameChar = fusedSatisfy isNameAsciiChar isNameChar isNameChar isNameChar >> pure ()

rawName :: Parser RawName
rawName = RawName <$> byteStringOf (nameStartChar >> many nameChar)

name :: Parser Name
name = NRawName <$> rawName

keywords :: FrozenDictionary V.Vector RawName V.Vector ()
keywords = unsafePerformIO (HT.fromList l >>= HT.unsafeFreeze)
 where
  l = (,()) <$> ["theory", "instance", "end"]

identOrKeyword :: Parser Token
identOrKeyword = do
  n <- rawName
  pure $ case HT.findElem keywords n of
    -1 -> IDENT $ NRawName n
    _ -> KEYWORD $ NRawName n

int :: Parser Token
int = INT <$> anyAsciiDecimalInt

lex1 :: Parser Token
lex1 =
  ws
    >> $( switch
            [|
              case _ of
                "(" -> pure LPAREN
                ")" -> pure RPAREN
                "[" -> pure LBRACK
                "]" -> pure RBRACK
                "{" -> pure LCURLY
                "}" -> pure RCURLY
                "," -> pure COMMA
                ";" -> pure SEMICOLON
                "\n" -> pure NL
                "'" -> TAG <$> name
                "." -> FIELD <$> name
                _ ->
                  identOrKeyword
                    <|> op
                    <|> int
                    <|> (eof >> pure EOF)
                    <|> pure ERROR
              |]
        )
