-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

-- | QuickCheck generators for FNotation source text.
module Test.FNotation.Property.Gen.Source (
  -- * Source text generator
  FNSource (..),
)
where

import Data.Text (Text)
import Data.Text qualified as T
import Test.QuickCheck

--------------------------------------------------------------------------------
-- FNotation source text generator
--------------------------------------------------------------------------------

{- | A newtype wrapper for generated FNotation source text that is suitable
for feeding to the lexer. The generated text exercises every lexer code path:

 * Alphanumeric identifiers (starting with letter or @_@, containing
   letters, digits, @_@, @-@)
 * Symbolic identifiers (sequences of @\< \> - + / * ~ : =@)
 * Qualified names (@a\/b\/c@, alphanumeric segments separated by @\/@)
 * Integer literals
 * String literals (@\"...\"@)
 * Field access (@.name@)
 * Tag literals (@\'name@)
 * Punctuation: @( ) [ ] { } , ;@
 * Newlines
 * Whitespace (spaces and tabs)
 * Comments (@# ...@)
 * Unexpected characters (to test error recovery)
-}
newtype FNSource = FNSource {getText :: Text}
  deriving (Show)

instance Arbitrary FNSource where
  arbitrary = sized \n -> FNSource . T.concat <$> genTokenList (max 1 n)
  shrink (FNSource t) =
    let ts = [deleteAt i t | i <- [0 .. T.length t - 1]]
        deleteAt i s =
          let (a, b) = T.splitAt i s
           in a <> T.drop 1 b
     in FNSource <$> ts

{- | Generate a list of textual token fragments that together form valid
(or intentionally invalid) FNotation source.
-}
genTokenList :: Int -> Gen [Text]
genTokenList n = do
  numTokens <- chooseInt (1, max 1 (n * 2))
  go numTokens
 where
  go 0 = pure []
  go remaining = do
    (tok, sep) <- genSourceToken
    rest <- go (remaining - 1)
    pure (tok : sep : rest)

{- | Generate a single source token fragment together with an appropriate
trailing separator. Returns @(token, separator)@.

The frequency weights ensure good coverage of all lexer paths while
biasing towards the most common constructs.
-}
genSourceToken :: Gen (Text, Text)
genSourceToken =
  frequency
    [ -- Alphanumeric identifiers: most common token type
      (15, withSep genAlphaIdent)
    , -- Symbolic identifiers/operators
      (8, withSep genSymIdent)
    , -- Qualified alphanumeric names (a/b, a/b/c)
      (5, withSep genQualifiedName)
    , -- Integer literals
      (6, withSep genIntLiteral)
    , -- String literals (self-delimiting, no separator needed)
      (4, noSep genStringLiteral)
    , -- Field access (.name)
      (4, noSep genFieldAccess)
    , -- Tag literal ('name)
      (4, noSep genTagLiteral)
    , -- Parentheses
      (4, noSep $ elements ["(", ")"])
    , -- Square brackets
      (4, noSep $ elements ["[", "]"])
    , -- Curly braces
      (3, noSep $ elements ["{", "}"])
    , -- Comma
      (3, noSep $ pure ",")
    , -- Semicolon
      (3, noSep $ pure ";")
    , -- Newline
      (5, noSep $ pure "\n")
    , -- Whitespace (spaces and tabs)
      (3, noSep genWhitespace)
    , -- Comments (# to end of line)
      (3, noSep genComment)
    , -- Unexpected characters (test error recovery)
      (1, withSep genUnexpectedChar)
    ]
 where
  withSep g = do
    tok <- g
    sep <- genSeparator
    pure (tok, sep)
  noSep g = do
    tok <- g
    pure (tok, "")

{- | Generate a separator between tokens. Usually a space, sometimes nothing,
a tab, or a newline.
-}
genSeparator :: Gen Text
genSeparator =
  frequency
    [ (5, pure " ")
    , (1, pure "\t")
    , (1, pure "\n")
    , (1, pure "  ")
    , (1, pure "")
    ]

{- | Alphanumeric identifier: starts with a letter or @_@, followed by
letters, digits, @_@, or @-@.

Covers: @isAlphaNumStart@ and @isAlphaNum@ in the lexer.
-}
genAlphaIdent :: Gen Text
genAlphaIdent = do
  first <- genAlphaStart
  restLen <- chooseInt (0, 8)
  rest <- vectorOf restLen genAlphaNumChar
  pure $ T.pack (first : rest)

-- | Starting character for alphanumeric names: letter or underscore.
genAlphaStart :: Gen Char
genAlphaStart =
  frequency
    [ (20, elements (['a' .. 'z'] ++ ['A' .. 'Z']))
    , (2, pure '_')
    ]

-- | Continuation character for alphanumeric names: letter, digit, @_@, or @-@.
genAlphaNumChar :: Gen Char
genAlphaNumChar =
  frequency
    [ (15, elements (['a' .. 'z'] ++ ['A' .. 'Z']))
    , (5, elements ['0' .. '9'])
    , (2, pure '_')
    , (1, pure '-')
    ]

{- | Symbolic identifier: a non-empty sequence of symbol characters.
Symbol chars: @\< \> - + / * ~ : =@

Note: @/@ is a symbol character, so it gets consumed greedily into
symbolic tokens. This means qualified names only work with alphanumeric
leading segments.
-}
genSymIdent :: Gen Text
genSymIdent = do
  len <- chooseInt (1, 4)
  T.pack <$> vectorOf len genSymbolChar

-- | A single symbol character as recognized by the lexer's @isSymbol@.
genSymbolChar :: Gen Char
genSymbolChar = elements ['<', '>', '-', '+', '/', '*', '~', ':', '=']

{- | Qualified alphanumeric name: two or more alphanumeric segments joined
by @/@. For example: @mul\/unitl@, @a\/b\/c@.

Qualification only works when segments are alphanumeric (since @/@ is a
symbol character, a symbolic segment would greedily consume the @/@).
-}
genQualifiedName :: Gen Text
genQualifiedName = do
  numSegs <- chooseInt (2, 4)
  segs <- vectorOf numSegs genAlphaSegment
  pure $ T.intercalate "/" segs

genAlphaSegment :: Gen Text
genAlphaSegment = do
  first <- genAlphaStart
  restLen <- chooseInt (0, 5)
  rest <- vectorOf restLen genAlphaNumChar
  pure $ T.pack (first : rest)

-- | Integer literal: a sequence of digits.
genIntLiteral :: Gen Text
genIntLiteral = do
  len <- chooseInt (1, 6)
  T.pack <$> vectorOf len (elements ['0' .. '9'])

{- | String literal: @\"contents\"@. The lexer reads until the closing @\"@
with no escape sequences.
-}
genStringLiteral :: Gen Text
genStringLiteral = do
  len <- chooseInt (0, 20)
  -- Content can be anything except '"' and '\0' (null terminates the lexer)
  contents <- vectorOf len genStringChar
  pure $ T.pack ('"' : contents ++ ['"'])

-- | Characters valid inside a string literal: anything except @\"@ and @\\0@.
genStringChar :: Gen Char
genStringChar =
  frequency
    [ (10, elements (['a' .. 'z'] ++ ['A' .. 'Z'] ++ ['0' .. '9']))
    , (3, elements [' ', '\t', '!', '?', '.', ',', ';', ':', '-', '+'])
    , (1, elements ['\n', '(', ')', '[', ']', '{', '}', '#', '\'', '/'])
    ]

{- | Field access: @.@ immediately followed by a name. The lexer handles
this specially — it advances past the dot, then lexes a name.
-}
genFieldAccess :: Gen Text
genFieldAccess = T.cons '.' <$> genFieldOrTagName

-- | Tag literal: @\'@ immediately followed by a name.
genTagLiteral :: Gen Text
genTagLiteral = T.cons '\'' <$> genFieldOrTagName

{- | Name suitable for field access or tag literal. Can start with either
an alphanumeric start char or a symbol char (the lexer's @nameSeg@
dispatches on both).
-}
genFieldOrTagName :: Gen Text
genFieldOrTagName =
  frequency
    [ (4, genAlphaIdent)
    , (1, genSymIdent)
    ]

{- | Whitespace: one or more spaces/tabs (not newlines, those are separate
tokens).
-}
genWhitespace :: Gen Text
genWhitespace = do
  len <- chooseInt (1, 4)
  T.pack <$> vectorOf len (elements [' ', '\t'])

{- | Comment: @#@ followed by non-newline characters, terminated by a newline.
The lexer's @comment@ function advances while @(/= \'\\n\')@ then consumes
the newline.
-}
genComment :: Gen Text
genComment = do
  len <- chooseInt (0, 30)
  body <- vectorOf len genCommentChar
  pure $ T.pack ('#' : body ++ ['\n'])

-- | Characters valid inside a comment body: anything except newline and null.
genCommentChar :: Gen Char
genCommentChar =
  frequency
    [ (10, elements (['a' .. 'z'] ++ ['A' .. 'Z'] ++ ['0' .. '9']))
    , (3, elements [' ', '\t', '!', '?', '.', ',', ';', ':', '-', '+', '*'])
    , (1, elements ['(', ')', '[', ']', '{', '}', '"', '\'', '/', '#'])
    ]

{- | Unexpected character: a character that doesn't match any lexer rule,
triggering the @unexpectedChar@ error path.

These are characters that are not: whitespace, newline, null, digits,
letters, underscore, symbol chars, punctuation, @#@, @\"@, @.@, or @\'@.
-}
genUnexpectedChar :: Gen Text
genUnexpectedChar =
  T.singleton
    <$> elements
      [ '@'
      , '\\'
      , '`'
      , '!'
      , '?'
      , '^'
      , '&'
      , '$'
      , '%'
      ]
