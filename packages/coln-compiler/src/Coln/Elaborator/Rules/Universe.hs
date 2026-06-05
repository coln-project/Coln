module Coln.Elaborator.Rules.Universe where

import Prelude hiding (lookup)
import Prettyprinter

import Coln.Common
import Coln.Core.Params
import Coln.Core.Value qualified as V
import Coln.Core.Syntax qualified as S
import Coln.Core.Memoed
import Coln.Core.Print
import Coln.Core.Evaluation
import Coln.Elaborator.Diagnostics
import Coln.Elaborator.Environment
import Coln.Elaborator.Judgment

formation :: Span -> Universe -> Judgment c
formation sp u = Typ sp \_ -> pure $ univ u
