module Geolog.Diagnostics where

import Data.Text qualified as T
import Data.Text.Unsafe qualified as TU
import Data.Vector.Unboxed qualified as UV
import Geolog.Common
import Geolog.Diagnostics.Code (Code)
import Prettyprinter
import Prettyprinter.Render.Text
import System.IO (Handle)

-- Files
--------------------------------------------------------------------------------

-- | A @File@ is used to display diagnostic messages.
-- Specifically, a @File@ is used to convert the @Span@ in a diagnostic message
-- into snippet of source code with the span underlined.
--
-- In order to do this, we need to convert the byte positions in the @Span@ into
-- line/column positions. This can be done fairly efficiently by binary search
-- through the vector of newline positions in the file, so we create this vector
-- whenever we open a file and store it in the @File@ record.
data File = File
  { name :: FilePath,
    contents :: T.Text,
    lineBreaks :: UV.Vector Pos
  }

newFile :: FilePath -> T.Text -> File
newFile x t = File x t (findLineBreaks t)

findLineBreaks :: T.Text -> UV.Vector Pos
findLineBreaks t = UV.unfoldr nextNewline (-1)
  where
    l = TU.lengthWord8 t
    nextNewline i
      | i == -1 = Just (-1, 0)
      | i < l = case TU.iter t i of
          TU.Iter '\n' j -> Just (i, i + j)
          TU.Iter _ j -> nextNewline (i + j)
      | i == l = Just (i, i + 1)
      | otherwise = Nothing

type LineNum = Int

lineStart :: File -> LineNum -> Pos
lineStart f l = (f.lineBreaks UV.! l) + 1

lineEnd :: File -> LineNum -> Pos
lineEnd f l = f.lineBreaks UV.! (l + 1)

lineSpan :: File -> LineNum -> Span
lineSpan f l = Span (lineStart f l) (lineEnd f l)

lineContents :: File -> LineNum -> T.Text
lineContents f l = sliceWord8 (lineStart f l) (lineEnd f l) f.contents

lineOf :: File -> Pos -> LineNum
lineOf f i =
  seq
    (0 <= i && i < TU.lengthWord8 f.contents || error "position out of bounds")
    (go 0 (UV.length f.lineBreaks - 1))
  where
    go l r
      | l == r = l
      | i < lineStart f m = go l m
      | i > lineEnd f m = go m r
      | otherwise = m
      where
        m = (l + r) `div` 2

repeated :: Int -> Char -> Doc ann
repeated n c
  | n == 0 = mempty
  | n == 1 = pretty c
  | otherwise = pretty (T.replicate n (T.singleton c))

linePretty :: Int -> LineNum -> Span -> T.Text -> Span -> Doc ann
linePretty numWidth l (Span ls le) t (Span s e) =
  vsep
    [ gutter <+> pretty t,
      gutter <+> repeated (s' - ls) ' ' <> repeated nc '^'
    ]
  where
    s' = max ls s
    e' = min le e
    nc = max 1 (e' - s')
    ln = fill numWidth $ pretty $ l + 1
    gutter = ln <+> "|"

numDigits :: Int -> Int
numDigits n = go (abs n)
  where
    go x
      | x < 10 = 1
      | otherwise = 1 + go (x `div` 10)

-- | This is the function used to display the source code for a @Span@.
linesPretty :: File -> Span -> Doc ann
linesPretty f sp@(Span s e) =
  vsep
    [ linePretty numWidth l (lineSpan f l) (lineContents f l) sp
    | l <- [ls .. le]
    ]
  where
    ls = lineOf f s
    le = lineOf f e
    numWidth = max (numDigits ls) (numDigits le)

-- Reporter
--------------------------------------------------------------------------------

-- | A @Reporter@ is a destination for diagnostic messages. Currently this is
-- very simplistic; it's just a byte sink. The @reporterFancy@ flag is in theory
-- used to configure whether colors should be used, but we don't even look at
-- that yet.
data Reporter = Reporter
  { handle :: Handle,
    fancy :: Bool
  }

-- Diagnostics
--------------------------------------------------------------------------------

data SourceLoc = SourceLoc
  { file :: File,
    span :: Span
  }

instance Pretty SourceLoc where
  pretty (SourceLoc f s) = linesPretty f s

data Note = Note
  { noteSourceLoc :: Maybe SourceLoc,
    noteMessage :: Maybe (Doc Ann)
  }

instance Pretty Note where
  pretty (Note loc _) = pretty loc

data Diagnostic = Diagnostic
  { code :: Code,
    notes :: [Note]
  }

instance Pretty Diagnostic where
  pretty d = vsep $ pretty d.code : (map pretty d.notes)

reportIO :: Reporter -> Diagnostic -> IO ()
reportIO r d = hPutDoc r.handle (hardline <> pretty d <> hardline)

type ReporterArg = (?reporter :: Reporter)
