-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

{-# LANGUAGE TypeAbstractions #-}

module Coln.Elaborator.Judgment where

import Data.Coerce (coerce)
import Data.Functor.Compose (Compose (Compose))

import Coln.Common
import Coln.Core.Conversion (defEq)
import Coln.Core.Memoed qualified as M
import Coln.Core.Params
import Coln.Core.Print (prtIn, shape)
import Coln.Core.Value qualified as V
import Coln.Elaborator.Diagnostics
import Coln.Elaborator.Environment
import Coln.Report

import Prettyprinter ((<+>))

newtype Typ c = Typ {elab :: ElabEnv c -> IO (M.Ty c)}

newtype Syn c = Syn {elab :: ElabEnv c -> IO (V.Ty N, M.El c)}

newtype Chk c = Chk {elab :: ElabEnv c -> V.Ty N -> IO (M.El c)}

data Judgment c where
  FromTyp :: Typ c -> Judgment c
  FromSyn :: Syn c -> Judgment c
  FromChk :: DDoc -> Chk c -> Judgment c

useIs :: (V.HasEvaluation c, Functor f) => (ElabEnv N -> f (M.El N)) -> ElabEnv c -> f (M.El c)
useIs @c f e = fmap change $ f e{target = TargetAnonymous}
 where
  change = case V.scase @c of
    SNominative -> id
    SDescriptive -> M.is

-- elimSyn :: (V.HasEvaluation c) => Span -> (ElabEnv N -> IO (V.Ty N, M.El N)) -> Judgment c
-- elimSyn sp = Syn sp . coerce . useIs . (coerce `asTypeOf` (Compose .))

-- descSyn :: (V.HasEvaluation c) => DDoc -> Span -> (ElabEnv D -> IO (V.Ty N, M.El D)) -> Judgment c
-- descSyn @c nd sp f = Syn sp $ case V.scase @c of
--   SNominative -> \e -> do
--     let msg = "cannot use an unnamed" <+> nd
--     failWith e.diagEnv sp RequiresName msg
--   SDescriptive -> f

intoTyp :: Span -> Judgment N -> Typ N
intoTyp _ (FromTyp t) = t
intoTyp sp (FromSyn s) = Typ $ \e -> do
  (ty, el) <- s.elab e
  case V.behavior ty of
    V.LikeU _ -> pure $ M.decode el
    _ -> do
      let msg = "tried to use a value of type" <+> prtIn e ty <+> "as a type"
      failWith e.diagEnv sp TypeMismatch msg
intoTyp _ (FromChk _ c) = Typ $ \e -> do
  el <- c.elab e $ V.U TheoryU
  pure $ M.decode el

intoSyn :: (V.HasEvaluation c) => DDoc -> Span -> Judgment c -> Syn c
intoSyn _ sp (FromTyp t) = Syn $ \e -> do
  raw <- t.elab e
  case universeFor (levelOf raw) of
    Nothing -> do
      let msg = "type" <+> prtIn e raw <+> "too large to fit in a universe"
      failWith e.diagEnv sp TypeTooLarge msg
    Just u -> pure (V.U u, M.code raw)
intoSyn _ _ (FromSyn s) = s
intoSyn use sp (FromChk nd _) = Syn $ \e -> do
  let msg = "Type annotation required when using a" <+> nd <+> "as" <+> use
  failWith e.diagEnv sp AnnotationRequired msg

intoChk :: (V.HasEvaluation c) => Span -> Judgment c -> Chk c
intoChk sp (FromTyp t) = Chk $ \e ty -> do
  raw <- t.elab e
  case V.behavior ty of
    V.LikeU u -> do
      case leq (levelOf raw) (decodesInto u) of
        True -> pure $ M.code raw
        False -> do
          let msg = "type" <+> prtIn e raw <+> "too large for universe" <+> pretty u
          failWith e.diagEnv sp TypeTooLarge msg
    _ -> do
      let msg = "cannot check type" <+> prtIn e raw <+> "at non-universe type" <+> prtIn e.scope ty
      failWith e.diagEnv sp TypeAtNonUniverse msg
intoChk sp (FromSyn s) = Chk $ \e ty -> do
  (ty', el) <- s.elab e
  case defEq (shape e) ty ty' of
    Right _ -> pure el
    Left err -> do
      let msg = "expected type" <+> prtIn e.scope ty <> ", but got type" <+> prtIn e.scope ty'
      let note = Just $ dpretty err
      failWithNote e.diagEnv sp TypeMismatch msg note
intoChk _ (FromChk _ c) = c

annotate :: Typ N -> Chk c -> Syn c
annotate t c = Syn \e -> do
  a <- t.elab (e{target = TargetAnonymous})
  m <- c.elab e a.val
  pure (a.val, m)
