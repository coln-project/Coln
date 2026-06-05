module Coln.Elaborator.Rules.Function where

import Coln.Common
import Coln.Core.Params
import Coln.Core.Value qualified as V
import Coln.Core.Syntax qualified as S
import Coln.Core.Memoed
import Coln.Core.Evaluation
import Coln.Elaborator.Diagnostics
import Coln.Elaborator.Environment
import Coln.Elaborator.Judgment
import Coln.Report

variantFor :: Ty N -> Ty N -> Span -> ElabEnv c -> IO FunctionVariant
variantFor dom cod sp e = case (levelOf dom, levelOf cod) of
  ((Set, Set); (Set, Theory)) -> pure SetTheory
  ((Set, Top); (Theory, _)) -> pure TheoryTop
  (Top, _) -> do
    let msg = "higher-order theories are not supported"
    failWith e.diagEnv sp FunctionDomainTooLarge msg

data Binder = Anonymous (Judgment N) | Named Name (Judgment N)

formation :: Span -> Binder -> Judgment N -> Judgment c
formation sp (Anonymous dom) cod = Typ sp $ \e -> do
  edom <- typ dom e
  ecod <- typ cod e
  v <- variantFor edom ecod sp e
  pure $ function e.scope.locals v edom (S.AbsConst ecod)
formation sp (Named x dom) cod = Typ sp $ \e -> do
  edom <- typ dom e
  ecod <- typ cod $ e { scope = bind x edom.val e.scope }
  v <- variantFor edom ecod sp e
  pure $ function e.scope.locals v edom (S.Abs x ecod)

intro :: (V.HasEvaluation c) => Span -> Name -> Judgment c -> Judgment c
intro sp x body = Chk "function abstraction" sp $ \e a ->
  case V.behavior a of
    V.LikeFunction ft -> do
      ebody <- withBound x ft.dom e.scope $ \v scope' ->
        chk body
          (e { scope = scope', target = appTarget e.target v })
          (V.appClo ft.cod v)
      pure $ lam e.scope.locals (fromVTy e.scope.len a) (S.Abs x ebody)
    _ -> do
      let msg = "tried to check a lambda expression at a non-function type"
      failWith e.diagEnv sp CheckLambdaAtNonFunctionType msg

elim :: (V.HasEvaluation c) => Span -> Judgment N -> Judgment N -> Judgment c
elim sp callee arg = elimSyn sp $ \e -> do
  (ty, ecallee) <- syn "head of a function call" callee e
  case V.behavior ty of
    V.LikeFunction ft -> do
      earg <- chk arg e ft.dom
      pure (V.appClo ft.cod earg.val, app ecallee earg)
    _ -> do
      let msg = "tried to apply a value that was not of a function type"
      failWith e.diagEnv sp ApplicationOfNonFunction msg
