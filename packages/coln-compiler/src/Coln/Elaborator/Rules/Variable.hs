module Coln.Elaborator.Rules.Variable where

import Prettyprinter
import Prelude hiding (lookup)

import Coln.Common
import Coln.Core.Evaluation
import Coln.Core.Globals
import Coln.Core.Memoed
import Coln.Core.Params
import Coln.Core.Syntax qualified as S
import Coln.Core.Value qualified as V
import Coln.Elaborator.Diagnostics
import Coln.Elaborator.Environment
import Coln.Elaborator.Judgment
import Coln.Report

find :: Span -> Name -> Syn N
find sp x = Syn \e ->
  case lookup e.scope x of
    Just (i, v, ty) -> pure (ty, localVar i v)
    Nothing -> case lookup e.globals x of
      Just ge -> pure (ge.ty, globalVar x ge.val)
      Nothing -> do
        let msg = "no such variable" <+> dpretty x <+> "in scope"
        failWith e.diagEnv sp VariableNotInScope msg
