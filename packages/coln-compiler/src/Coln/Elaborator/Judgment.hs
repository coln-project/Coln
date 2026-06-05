{-# LANGUAGE TypeAbstractions #-}
module Coln.Elaborator.Judgment where

import Data.Coerce (coerce)
import Data.Functor.Compose (Compose(Compose))

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

data Judgment c where
  Typ :: Span -> (ElabEnv N -> IO (M.Ty N)) -> Judgment c
  Syn :: Span -> (ElabEnv c -> IO (V.Ty N, M.El c)) -> Judgment c
  Chk :: DDoc -> Span -> (ElabEnv c -> V.Ty N -> IO (M.El c)) -> Judgment c

useIs :: (V.HasEvaluation c, Functor f) => (ElabEnv N -> f (M.El N)) -> ElabEnv c -> f (M.El c)
useIs @c f e = fmap change $ f e { target = TargetAnonymous }
  where
    change = case V.scase @c of
      SNominative -> id
      SDescriptive -> M.is

elimSyn :: (V.HasEvaluation c) => Span -> (ElabEnv N -> IO (V.Ty N, M.El N)) -> Judgment c
elimSyn sp = Syn sp . coerce . useIs . (coerce `asTypeOf` (Compose .))

descSyn :: (V.HasEvaluation c) => DDoc -> Span -> (ElabEnv D -> IO (V.Ty N, M.El D)) -> Judgment c
descSyn @c nd sp f = Syn sp $ case V.scase @c of
  SNominative -> \e -> do
    let msg = "cannot use an unnamed" <+> nd
    failWith e.diagEnv sp RequiresName msg
  SDescriptive -> f

typ :: Judgment N -> ElabEnv N -> IO (M.Ty N)
typ (Typ _ f) e = f e
typ (Syn sp f) e = do
  (ty, el) <- f e
  case V.behavior ty of
    V.LikeU _ -> pure $ M.decode el
    _ -> do
      let msg = "tried to use an ordinary value as a type"
      failWith e.diagEnv sp TypeMismatch msg
typ (Chk _ _ f) e = do
  el <- f e $ V.U TheoryU
  pure $ M.decode el

syn :: (V.HasEvaluation c) => DDoc -> Judgment c -> ElabEnv c -> IO (V.Ty N, M.El c)
syn @c _ (Typ sp f) e = do
  raw <- f (e { target = TargetAnonymous })
  case universeFor (levelOf raw) of
    Nothing -> do
      let msg = "type" <+> prtIn e raw <+> "too large to fit in a universe"
      failWith e.diagEnv sp TypeTooLarge msg
    Just u -> case V.scase @c of
      SNominative -> pure $ (V.U u, M.code raw)
      SDescriptive -> pure $ (V.U u, M.is $ M.code raw)
syn _ (Syn _ f) e = f e
syn use (Chk nd sp _) e = do
  let msg = "Type annotation required when using a" <+> nd <+> "as" <+> use
  failWith e.diagEnv sp AnnotationRequired msg

chk :: (V.HasEvaluation c) => Judgment c -> ElabEnv c -> V.Ty N -> IO (M.El c)
chk @c (Typ sp f) e ty = do
  raw <- f (e { target = TargetAnonymous })
  case V.behavior ty of
    V.LikeU u -> do
      case leq (levelOf raw) (decodesInto u) of
        True -> case V.scase @c of
          SNominative -> pure $ M.code raw
          SDescriptive -> pure $ M.is $ M.code raw
        False -> do
          let msg = "type" <+> prtIn e raw <+> "too large for universe" <+> pretty u
          failWith e.diagEnv sp TypeTooLarge msg
    _ -> do
      let msg = "cannot check type" <+> prtIn e raw <+> "at non-universe type" <+> prtIn e.scope ty
      failWith e.diagEnv sp TypeAtNonUniverse msg
chk (Syn sp f) e ty = do
  (ty', el) <- f e
  case defEq (shape e) ty ty' of
    Right _ -> pure el
    Left err -> do
      let msg = "expected type" <+> prtIn e.scope ty <> ", but got type" <+> prtIn e.scope ty'
      let note = Just $ dpretty err
      failWithNote e.diagEnv sp TypeMismatch msg note
chk (Chk _ _ f) e ty = f e ty
