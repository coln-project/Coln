module Diagnostician.HTML (
  diagnosticToHtml,
) where

import Data.Text (Text)
import Diagnostician
import Lucid (Html, class_, div_, span_)
import Prettyprinter (defaultLayoutOptions, layoutPretty)
import Prettyprinter.Lucid (renderHtml)
import Prettyprinter.Render.Util.SimpleDocTree (treeForm)

-- | Render a diagnostic to some HTML with classes for annotations.
diagnosticToHtml :: (Code a) => Diagnostic a -> Html ()
diagnosticToHtml d =
  div_
    [class_ ("diag-" <> severityClassSuffix (codeMeta d.code).severity)]
    . renderHtml
    . treeForm
    . fmap (span_ . pure @[] . class_ . ("ann-" <>) . annClassSuffix)
    . layoutPretty defaultLayoutOptions
    $ dpretty d

severityClassSuffix :: Severity -> Text
severityClassSuffix = \case
  SDebug -> "debug"
  SInfo -> "info"
  SWarning -> "warning"
  SError -> "error"

annClassSuffix :: DiagnosticAnn -> Text
annClassSuffix = \case
  DSeverity -> "severity"
  DCode -> "code"
  DBar -> "bar"
  DSpan -> "span"
