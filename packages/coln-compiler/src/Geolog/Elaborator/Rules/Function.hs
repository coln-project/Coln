module Geolog.Elaborator.Rules.Function where

import Geolog.Common
import Geolog.Core.Params
import Geolog.Core.Value qualified as V
import Geolog.Core.Syntax qualified as S
import Geolog.Core.Memoed
import Geolog.Core.Evaluation
import Geolog.Elaborator.Diagnostics
import Geolog.Elaborator.Environment
import Geolog.Elaborator.Judgment
import Geolog.Report

data Binder = Anonymous (Judgment N) | Named Name (Judgment N)

formation :: Binder -> Judgment N -> Judgment c
formation (Anonymous dom) cod = Typ $ \e -> do
  edom <- typ dom e
  ecod <- typ cod e
  pure $ function e.scope.locals edom (S.AbsConst ecod)
formation (Named x dom) cod = nTyp $ \e -> do
  edom <- typ dom e
  ecod <- typ cod $ e { scope = bind x edom.val e.scope }
  pure $ function e.scope.locals edom (S.Abs x ecod)

intro :: (V.HasEvaluation c) => Span -> Name -> Judgment c -> Judgment c
intro sp x body = Chk $ \e a ->
  case V.behavior a of
    V.LikeFunction ft -> do
      ebody <- withBound x ft.dom e.scope $ \v scope' ->
        body.elab
          (e { scope = scope', target = appTarget e.target v })
          (V.appClo ft.cod v)
      pure $ lam e.scope.locals (fromVTy e.scope.len a) (S.Abs x ebody)
    _ -> do
      let msg = "tried to check a lambda expression at a non-function type"
      failWith e.diagEnv sp CheckLambdaAtNonFunctionType msg

elim :: (V.HasEvaluation c) => Span -> Judgment N -> Judgment N -> Judgment c
elim sp callee arg = elimSyn $ \e -> do
  (ecallee, a) <- syn callee e
  case V.behavior a of
    V.LikeFunction ft -> do
      earg <- chk arg e ft.dom
      pure (app ecallee earg, V.appClo ft.cod earg.val)
    _ -> do
      let msg = "tried to apply a value that was not of a function type"
      failWith e.diagEnv sp ApplicationOfNonFunction msg
