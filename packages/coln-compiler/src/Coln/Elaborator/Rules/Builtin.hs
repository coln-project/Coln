module Coln.Elaborator.Rules.Builtin where

import Coln.Common
import Coln.Core.Params
import Coln.Core.Memoed
import Coln.Core.Value qualified as V
import Coln.Elaborator.Judgment

formation :: Span -> BuiltinTy -> Judgment c
formation sp bt = Typ sp $ \_ -> do
  pure $ builtinTy bt

intro :: (V.HasEvaluation c) => Span -> Literal -> Judgment c
intro sp l = elimSyn sp $ \_ -> case l of
  LitInt _ -> pure (V.BuiltinTy BuiltinInt, lit l)
  LitString _ -> pure (V.BuiltinTy BuiltinString, lit l)
