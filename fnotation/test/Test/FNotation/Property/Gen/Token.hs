{- | QuickCheck generators for FNotation token streams.

These generators produce 'V.Vector Token' values that satisfy the invariants
expected by the parser ('FNotation.Parser.parse'):

  1. Every token has the correct 'Kind'/'TokenValue' pairing:

       * 'AIdent', 'AKeyword', 'SIdent', 'SKeyword', 'Decl', 'Block', 'End',
         'Field', 'Tag' carry a 'VName'.
       * 'Int' carries a 'VInt'.
       * 'String' carries a 'VString'.
       * All punctuation and structural kinds ('LParen', 'RParen', 'LBrack',
         'RBrack', 'LCurly', 'RCurly', 'Comma', 'Semicolon', 'Nl', 'Eof',
         'Error') carry 'VEmpty'.

  2. The stream is non-empty and always ends with an 'Eof' token (the parser's
     @stmts@ loop terminates only on 'End' or 'Eof').

  3. Spans are synthetic but well-formed (non-negative, start <= end), assigned
     sequentially so they don't overlap.
-}
module Test.FNotation.Property.Gen.Token (
  -- * Token stream wrapper
  FNTokens (..),

  -- * Individual token generators
  genToken,
  genNameToken,
  genPunctToken,
)
where

import Data.Text (Text)
import Data.Text qualified as T
import Data.Vector qualified as V
import Diagnostician (Span (..))
import FNotation.Names (Name (..))
import FNotation.Tokens (Kind (..), Token (..), TokenValue (..))
import Test.QuickCheck

--------------------------------------------------------------------------------
-- Token stream newtype
--------------------------------------------------------------------------------

{- | A newtype wrapper around a non-empty 'V.Vector' of 'Token's that is
always terminated by 'Eof'. Suitable for feeding directly to
'FNotation.Parser.parse'.
-}
newtype FNTokens = FNTokens {getTokens :: V.Vector Token}

instance Show FNTokens where
  show (FNTokens ts) =
    "FNTokens ("
      ++ show (V.length ts)
      ++ " tokens): "
      ++ show (V.toList (V.map (\t -> t.kind) ts))

instance Arbitrary FNTokens where
  arbitrary = sized \n -> do
    numTokens <- chooseInt (0, max 0 (n * 3))
    toks <- genTokenStream numTokens
    pure (FNTokens toks)

  shrink (FNTokens ts)
    | V.length ts <= 1 = []
    | otherwise =
        -- Remove one non-Eof token at a time, re-assign spans
        [ FNTokens (reassignSpans (V.ifilter (\i _ -> i /= j) ts))
        | j <- [0 .. V.length ts - 2]
        ]

--------------------------------------------------------------------------------
-- Token stream generation
--------------------------------------------------------------------------------

{- | Generate a complete token stream: a sequence of arbitrary tokens
followed by an 'Eof' sentinel. Spans are assigned sequentially.
-}
genTokenStream :: Int -> Gen (V.Vector Token)
genTokenStream n = do
  toks <- vectorOf n genToken
  let eof = Token Eof VEmpty dummySpan
  pure $ reassignSpans (V.fromList (toks ++ [eof]))

{- | Re-assign spans to a token vector so they are sequential and
non-overlapping. Each token gets a 1-unit span, placed end-to-end.
-}
reassignSpans :: V.Vector Token -> V.Vector Token
reassignSpans = V.imap \i tok -> tok{span = Span i (i + 1)}

-- | A dummy span used as a placeholder before 'reassignSpans' fixes them up.
dummySpan :: Span
dummySpan = Span 0 0

--------------------------------------------------------------------------------
-- Individual token generators
--------------------------------------------------------------------------------

{- | Generate a single token with a well-formed kind/value pairing.

The frequency distribution covers every 'Kind' constructor. Kinds that the
parser actively dispatches on ('AIdent', 'SIdent', 'LParen', 'LBrack', 'Nl',
etc.) are weighted more heavily to produce interesting parse trees.
-}
genToken :: Gen Token
genToken =
  frequency
    [ -- Name-bearing tokens (parser dispatches on these in arg/expr/stmt)
      (10, genNameToken AIdent)
    , (3, genNameToken AKeyword)
    , (6, genNameToken SIdent)
    , (3, genNameToken SKeyword)
    , (3, genNameToken Decl)
    , (3, genNameToken Block)
    , (2, genNameToken End)
    , (3, genNameToken Field)
    , (3, genNameToken Tag)
    , -- Int literals
      (4, genIntToken)
    , -- String literals
      (3, genStringToken)
    , -- Punctuation / structural
      (5, genPunctToken LParen)
    , (5, genPunctToken RParen)
    , (4, genPunctToken LBrack)
    , (4, genPunctToken RBrack)
    , (2, genPunctToken LCurly)
    , (2, genPunctToken RCurly)
    , (3, genPunctToken Comma)
    , (2, genPunctToken Semicolon)
    , (6, genPunctToken Nl)
    , -- Error token (parser may encounter these from lexer errors)
      (1, genPunctToken Error)
    ]

-- | Generate a token whose kind requires a 'VName' payload.
genNameToken :: Kind -> Gen Token
genNameToken k = do
  n <- genName
  pure $ Token k (VName n) dummySpan

-- | Generate a token with 'VEmpty' payload (punctuation, structural, error).
genPunctToken :: Kind -> Gen Token
genPunctToken k = pure $ Token k VEmpty dummySpan

-- | Generate an integer literal token.
genIntToken :: Gen Token
genIntToken = do
  i <- chooseInt (0, 9999)
  pure $ Token Int (VInt i) dummySpan

-- | Generate a string literal token.
genStringToken :: Gen Token
genStringToken = do
  t <- genShortText
  pure $ Token String (VString t) dummySpan

--------------------------------------------------------------------------------
-- Name generation
--------------------------------------------------------------------------------

{- | Generate a 'Name'. Names have zero or more qualifying segments and a
final segment. Each segment is a short alphabetic string.
-}
genName :: Gen Name
genName =
  frequency
    [ (4, Name [] <$> genLastSegment)
    , (2, Name <$> listOf1to3 genQualSegment <*> genLastSegment)
    ]
 where
  listOf1to3 g = do
    n <- chooseInt (1, 3)
    vectorOf n g

{- | Generate the final segment of a name. This determines the name's
classification (identifier vs keyword, alphanumeric vs symbolic), so we
generate both kinds.
-}
genLastSegment :: Gen Text
genLastSegment =
  frequency
    [ (4, genAlphaSegment)
    , (1, genSymSegment)
    ]

{- | Qualifying segments are always alphanumeric (symbolic segments
greedily consume @/@).
-}
genQualSegment :: Gen Text
genQualSegment = genAlphaSegment

{- | A short alphanumeric name segment: starts with a letter or @_@,
followed by letters, digits, @_@, or @-@.
-}
genAlphaSegment :: Gen Text
genAlphaSegment = do
  first <- genAlphaStart
  restLen <- chooseInt (0, 5)
  rest <- vectorOf restLen genAlphaNumChar
  pure $ T.pack (first : rest)

-- | Starting character for alphanumeric segments.
genAlphaStart :: Gen Char
genAlphaStart =
  frequency
    [ (20, elements (['a' .. 'z'] ++ ['A' .. 'Z']))
    , (2, pure '_')
    ]

-- | Continuation character for alphanumeric segments.
genAlphaNumChar :: Gen Char
genAlphaNumChar =
  frequency
    [ (15, elements (['a' .. 'z'] ++ ['A' .. 'Z']))
    , (5, elements ['0' .. '9'])
    , (2, pure '_')
    , (1, pure '-')
    ]

{- | A short symbolic name segment: a non-empty sequence of symbol characters
(@\< \> - + \/ * ~ : =@).
-}
genSymSegment :: Gen Text
genSymSegment = do
  len <- chooseInt (1, 3)
  T.pack <$> vectorOf len (elements ['<', '>', '-', '+', '/', '*', '~', ':', '='])

-- | A short arbitrary text for string literal payloads.
genShortText :: Gen Text
genShortText = do
  len <- chooseInt (0, 15)
  T.pack <$> vectorOf len arbitraryPrintableChar
