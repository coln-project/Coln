module Coln.Elaborator.Rules.Universe where

import Prettyprinter
import Prelude hiding (lookup)

import Coln.Common
import Coln.Core.Evaluation
import Coln.Core.Memoed
import Coln.Core.Params
import Coln.Core.Print
import Coln.Core.Syntax qualified as S
import Coln.Core.Value qualified as V
import Coln.Elaborator.Diagnostics
import Coln.Elaborator.Environment
import Coln.Elaborator.Judgment

formation :: Universe -> Typ N
formation u = Typ \_ -> pure $ univ u
