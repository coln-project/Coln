module Geolog.Diagnostics where

import Data.ByteString (ByteString)
import Data.ByteString.Builder
-- import Data.ByteString qualified as BS
import Data.Hashable
import Data.Vector (Vector)
import Data.Vector.Hashtables
import Data.Vector.Strict.Mutable qualified as VM
import Data.Vector.Unboxed.Mutable qualified as UM
import Geolog.Common
import Geolog.Diagnostics.Code (Code)
import Geolog.Diagnostics.Code qualified as Code
import Lens.Micro.TH (makeFields)
import System.IO (Handle)

data FileLoc = Path FilePath | Memory

data File = File
  { fileName :: FileLoc,
    fileContents :: ByteString,
    fileNewlines :: Vector BytePos
  }

makeFields ''File

newtype FileId = FileId Int
  deriving (Eq, Hashable) via Int

data FileSystem = FileSystem
  {fileSystemLookup :: Dictionary RealWorld UM.MVector FileId VM.MVector File}

makeFields ''FileSystem

data Reporter = Reporter
  { reporterFileSystem :: FileSystem,
    reporterHandle :: Handle,
    reporterFancy :: Bool
  }

makeFields ''Reporter

data Note = Note
  { noteFileId :: FileId,
    noteLoc :: SourceLoc,
    noteMessage :: Maybe Builder
  }

makeFields ''Note

data Severity = Debug | Info | Warning | Error

severity :: Code -> Severity
severity Code.UnexpectedCharacter = Error
severity Code.UnexpectedToken = Error

shortcode :: Code -> Builder
-- codes 0-100 are for lexing
shortcode Code.UnexpectedCharacter = "E0000"
-- codes 100-200 are for parsing
shortcode Code.UnexpectedToken = "E0100"

data Diagnostic = Diagnostic
  { diagnosticCode :: Code,
    diagnosticNotes :: [Note]
  }

makeFields ''Diagnostic

report :: Reporter -> Diagnostic -> IO ()
report _ _ = pure ()
