module Geolog.Diagnostics where

import Data.Text qualified as T
import Data.Text.Unsafe qualified as TU
import Data.Vector.Unboxed qualified as UV
import Geolog.Common
import Geolog.Diagnostics.Code (Code)
import Lens.Micro.Platform (makeFields, (^.))
import Prettyprinter
import Prettyprinter.Render.Text
import System.IO (Handle)

data File = File
  { fileName :: FilePath,
    fileContents :: T.Text,
    fileLineBreaks :: UV.Vector Pos
  }

makeFields ''File

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
lineStart f l = ((f ^. lineBreaks) UV.! l) + 1

lineEnd :: File -> LineNum -> Pos
lineEnd f l = (f ^. lineBreaks) UV.! (l + 1)

lineSpan :: File -> LineNum -> Span
lineSpan f l = Span (lineStart f l) (lineEnd f l)

lineContents :: File -> LineNum -> T.Text
lineContents f l = sliceWord8 (lineStart f l) (lineEnd f l) (f ^. contents)

lineOf :: File -> Pos -> LineNum
lineOf f i = go (UV.length (f ^. lineBreaks) `div` 2)
  where
    go l
      | i < lineStart f l = go (l `div` 2)
      | i > lineEnd f l = go (l + l `div` 2)
      | otherwise = l

linesFor :: File -> Span -> [(LineNum, Span, T.Text)]
linesFor f (Span s e) =
  [ (l, lineSpan f l, lineContents f l)
    | l <- [lineOf f s .. lineOf f e]
  ]

repeated :: Int -> Char -> Doc ann
repeated n c
  | n == 0 = mempty
  | n == 1 = pretty c
  | otherwise = pretty (T.replicate n (T.singleton c))

linePretty :: LineNum -> Span -> T.Text -> Span -> Doc ann
linePretty l (Span ls le) t (Span s e) =
  vsep
    [ gutter <+> pretty t,
      gutter <+> repeated (s' - ls) ' ' <> repeated nc '^'
    ]
  where
    s' = max ls s
    e' = min le e
    nc = min 1 (e' - s')
    ln = fill 4 $ pretty $ l + 1
    gutter = ln <+> "|"

linesPretty :: File -> Span -> Doc ann
linesPretty f sp@(Span s e) =
  vsep
    [ linePretty l (lineSpan f l) (lineContents f l) sp
      | l <- [lineOf f s .. lineOf f e]
    ]

data Reporter = Reporter
  { reporterHandle :: Handle,
    reporterFancy :: Bool
  }

makeFields ''Reporter

data SourceLoc = SourceLoc
  { sourceLocFile :: File,
    sourceLocSpan :: Span
  }

instance Pretty SourceLoc where
  pretty (SourceLoc f s) = linesPretty f s

data Note = Note
  { noteSourceLoc :: Maybe SourceLoc,
    noteMessage :: Maybe (Doc Ann)
  }

makeFields ''Note

instance Pretty Note where
  pretty (Note loc _) = pretty loc

data Diagnostic = Diagnostic
  { diagnosticCode :: Code,
    diagnosticNotes :: [Note]
  }

makeFields ''Diagnostic

instance Pretty Diagnostic where
  pretty d = vsep $ pretty (d ^. code) : (map pretty (d ^. notes))

report :: Reporter -> Diagnostic -> IO ()
report r d = hPutDoc (r ^. handle) (hardline <> pretty d <> hardline)
