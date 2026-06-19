module Coln.Elaborator.Rules.Function where

import Coln.Common
import Coln.Core.Evaluation
import Coln.Core.Memoed
import Coln.Core.Params
import Coln.Core.Syntax qualified as S
import Coln.Core.Value qualified as V
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

data Binder = Anonymous (Typ N) | Named Name (Typ N)

formation :: Span -> Binder -> Typ N -> Typ N
formation sp (Anonymous dom) cod = Typ \e -> do
  edom <- dom.elab e
  ecod <- cod.elab e
  v <- variantFor edom ecod sp e
  pure $ function e.scope.locals v edom (S.AbsConst ecod)
formation sp (Named x dom) cod = Typ \e -> do
  edom <- dom.elab e
  ecod <- cod.elab $ e{scope = bind x edom.val e.scope}
  v <- variantFor edom ecod sp e
  pure $ function e.scope.locals v edom (S.Abs x ecod)

intro :: (V.HasEvaluation c) => Span -> Name -> Chk c -> Chk c
intro sp x body = Chk \e a ->
  case V.behavior a of
    V.LikeFunction ft -> do
      ebody <- withBound x ft.dom e.scope $ \v scope' ->
        body.elab
          (e{scope = scope', target = appTarget e.target v})
          (V.appClo ft.cod v)
      pure $ lam e.scope.locals (fromVTy e.scope.len a) (S.Abs x ebody)
    _ -> do
      let msg = "tried to check a lambda expression at a non-function type"
      failWith e.diagEnv sp CheckLambdaAtNonFunctionType msg

elim :: Span -> Syn N -> Chk N -> Syn N
elim sp callee arg = Syn $ \e -> do
  (ty, ecallee) <- callee.elab e
  case V.behavior ty of
    V.LikeFunction ft -> do
      earg <- arg.elab e ft.dom
      pure (V.appClo ft.cod earg.val, app ecallee earg)
    _ -> do
      let msg = "tried to apply a value that was not of a function type"
      failWith e.diagEnv sp ApplicationOfNonFunction msg
