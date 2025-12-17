module Geolog.Lexer where

import Data.Char (isLetter)
import FlatParse.Basic hiding (Parser, runParser)
import Geolog.Common
import Geolog.Notation
import Symbolize qualified
import Prelude hiding (lookup)

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

ops :: ConfTable OpData
ops =
  fromList
    [ (":", KwOp (NonAssoc 20))
    ]

op :: Parser Token
op = do
  n <- Symbolize.intern <$> byteStringOf (some opChar)
  case lookup ops n of
    Nothing -> error "no registered precedence"
    Just (KwOp p) -> pure $ KEYWORD_OP n p
    Just (UserOp p) -> pure $ OP n p

nameChar :: Parser ()
nameChar = fusedSatisfy isNameAsciiChar isNameChar isNameChar isNameChar >> pure ()

name :: Parser Name
name = Symbolize.intern <$> byteStringOf (nameStartChar >> many nameChar)

data SpecialNameKind
  = K_Block
  | K_Decl
  | K_Keyword
  | K_End

specialNames :: ConfTable SpecialNameKind
specialNames =
  fromList
    [ ("theory", K_Block)
    , ("instance", K_Block)
    , ("def", K_Decl)
    , ("let", K_Decl)
    , ("end", K_End)
    ]

fromName :: Name -> Token
fromName n = case lookup specialNames n of
  Nothing -> IDENT n
  Just K_Block -> BLOCK n
  Just K_Decl -> DECL n
  Just K_Keyword -> KEYWORD n
  Just K_End -> END

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
                  (fromName <$> name)
                    <|> op
                    <|> int
                    <|> (eof >> pure EOF)
                    <|> pure ERROR
              |]
        )
