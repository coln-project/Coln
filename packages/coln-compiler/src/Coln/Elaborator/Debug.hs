-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Elaborator.Debug where

import Prettyprinter

import Coln.Common
import Coln.Core.Conversion
import Coln.Core.Evaluation
import Coln.Core.Memoed
import Coln.Core.Params
import Coln.Core.Print
import Coln.Core.Syntax qualified as S
import Coln.Core.Value qualified as V
import Coln.Elaborator.Diagnostics
import Coln.Elaborator.Environment
import Coln.Elaborator.Judgment
import Coln.Report

data DebugCommand
  = ShowType Span (Syn N)
  | ShowTypeBehavior Span (Syn N)
  | ShowLevel Span (Typ N)
  | Expand Span (Syn N)

runDebug :: ElabEnv N -> DebugCommand -> IO ()
runDebug e (ShowType sp s) = do
  (a, m) <- s.elab e
  report e.diagEnv sp DebugMisc ("value" <+> prtIn e m.val <+> "has type" <+> prtIn e a)
runDebug e (ShowTypeBehavior sp s) = do
  (a, m) <- s.elab e
  report e.diagEnv sp DebugMisc ("value" <+> prtIn e m.val <+> "has type" <+> prtIn e a <+> "with behavior" <+> prtIn e (V.behavior a))
runDebug e (ShowLevel sp s) = do
  ty <- s.elab e
  report e.diagEnv sp DebugMisc ("type" <+> prtIn e ty <+> "has level" <+> (pretty $ show $ levelOf ty))
runDebug e (Expand sp s) = do
  (_, m) <- s.elab e
  report e.diagEnv sp DebugMisc ("expands to:" <+> prtIn e (canon m.val))
