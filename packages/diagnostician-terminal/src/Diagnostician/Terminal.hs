module Diagnostician.Terminal (
  terminalReporter,
) where

import Data.Text.Lazy.IO qualified as TL
import Diagnostician
import Prettyprinter (defaultLayoutOptions, layoutPretty)
import Prettyprinter.Render.Terminal qualified as Ansi
import Prettyprinter.Render.Text qualified as Text
import System.Console.ANSI (hSupportsANSI)
import System.IO (Handle)

-- | Writes diagnostics to the handle, containing ANSI colour escape codes only if the handle refers to a terminal.
terminalReporter :: (Code a) => Handle -> Reporter a
terminalReporter handle =
  Reporter
    { reportIO = \d -> do
        isTerm <- hSupportsANSI handle
        let render =
              if isTerm
                then Ansi.renderLazy . fmap (toAnsiStyle (codeMeta d.code).severity)
                else Text.renderLazy
        TL.hPutStr handle $ render (layoutPretty defaultLayoutOptions $ dpretty d) <> "\n"
    }

toAnsiStyle :: Severity -> DiagnosticAnn -> Ansi.AnsiStyle
toAnsiStyle severity = \case
  DSeverity; DSpan -> Ansi.color colour
  DCode; DBar -> Ansi.colorDull colour
 where
  colour = case severity of
    SDebug -> Ansi.Magenta
    SInfo -> Ansi.Blue
    SWarning -> Ansi.Yellow
    SError -> Ansi.Red
