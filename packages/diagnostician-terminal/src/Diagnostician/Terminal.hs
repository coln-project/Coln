-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Diagnostician.Terminal (
  terminalReporter,
) where

import Diagnostician
import Prettyprinter (hardline)
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
                then Ansi.putDoc . fmap (toAnsiStyle (codeMeta d.code).severity)
                else Text.putDoc
        render $ dpretty d <> hardline
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
