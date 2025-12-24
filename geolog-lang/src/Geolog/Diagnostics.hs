module Geolog.Diagnostics where

import Data.ByteString (ByteString)
import Data.ByteString qualified as BS
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
    fileContents :: ByteString,
    fileNewlines :: Vector Int
  }

makeFields ''File

newFile :: FilePath -> ByteString -> File
newFile x bs = File x bs (V.fromList $ BS.elemIndices 10 bs)

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
