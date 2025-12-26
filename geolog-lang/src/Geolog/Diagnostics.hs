module Geolog.Diagnostics where

import Data.Text qualified as T
import Data.Vector (Vector)
import Data.Vector qualified as V
import Geolog.Common
import Geolog.Diagnostics.Code (Code)
import Lens.Micro.Platform (makeFields, (^.))
import Prettyprinter
import Prettyprinter.Render.Text
import System.IO (Handle)

data File = File
  { fileName :: FilePath,
    fileContents :: T.Text,
    fileNewlines :: Vector Pos
  }

makeFields ''File

newFile :: FilePath -> T.Text -> File
newFile x t = File x t (V.fromList [])

-- Given a Span, we want to
--
-- 1. Figure out which lines it covers
-- 2. Extract those lines as Text
-- 3. Create `  ^^^  ` annotations under the lines for the parts
--    of the lines that are covered by the Span.
-- 4. Print line numbers in the margins

data Reporter = Reporter
  { reporterHandle :: Handle,
    reporterFancy :: Bool
  }

makeFields ''Reporter

data SourceLoc = SourceLoc
  { sourceLocFile :: File,
    sourceLocSpan :: Span
  }

data Note = Note
  { noteSourceLoc :: Maybe SourceLoc,
    noteMessage :: Maybe (Doc Ann)
  }

makeFields ''Note

instance Pretty Note where
  pretty _ = mempty

data Diagnostic = Diagnostic
  { diagnosticCode :: Code,
    diagnosticNotes :: [Note]
  }

makeFields ''Diagnostic

instance Pretty Diagnostic where
  pretty d = vsep $ pretty (d ^. code) : (map pretty (d ^. notes))

report :: Reporter -> Diagnostic -> IO ()
report r d = hPutDoc (r ^. handle) (pretty d)
