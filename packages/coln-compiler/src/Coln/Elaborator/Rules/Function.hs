-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Elaborator.Rules.Function where

import Coln.Common
import Coln.Core
import Coln.Core.Syntax qualified as S
import Coln.Core.Value qualified as V
import Coln.Elaborator.Judgment

variantFor :: Mode -> Ty N -> Ty N -> Span -> ElabEnv c -> IO FunctionVariant
variantFor m dom cod sp e =
  case functionMLevelFor (levelOf dom).mlevel (levelOf cod).mlevel of
    Just l -> pure (FunctionVariant l (levelOf cod).hlevel m)
    Nothing -> do
      let msg = "higher-order theories are not supported"
      failWith e.diagEnv sp FunctionDomainTooLarge msg

data Binder = Anonymous Mode (Typ N) | Named Mode Name (Typ N)

shiftToMode :: Mode -> Scope -> Scope
shiftToMode Conjunctive sc = sc
shiftToMode Inductive sc = unlock sc

formation :: Span -> Binder -> Typ N -> Typ N
formation sp (Anonymous m dom) cod = Typ \e -> do
  edom <- dom.elab (e{scope = shiftToMode m e.scope})
  ecod <- cod.elab e
  v <- variantFor m edom ecod sp e
  pure $ function e.scope.locals v edom (S.AbsConst ecod)
formation sp (Named m x dom) cod = Typ \e -> do
  edom <- dom.elab (e{scope = shiftToMode m e.scope})
  ecod <- cod.elab $ e{scope = bind x edom.val m e.scope}
  v <- variantFor m edom ecod sp e
  pure $ function e.scope.locals v edom (S.Abs x ecod)

intro :: (V.HasEvaluation c) => Span -> Name -> Chk c -> Chk c
intro sp x body = Chk \e a ->
  case V.behavior a of
    V.LikeFunction ft -> do
      ebody <- withBound x ft.dom ft.variant.domainMode e.scope $ \v scope' ->
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
      earg <- arg.elab (e{scope = shiftToMode ft.variant.domainMode e.scope}) ft.dom
      pure (V.appClo ft.cod earg.val, app ecallee earg)
    _ -> do
      let msg = "tried to apply a value that was not of a function type"
      failWith e.diagEnv sp ApplicationOfNonFunction msg
