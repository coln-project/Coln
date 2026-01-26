{- |
We have three things to deal with:

1. Local context
2. Global context
3. Scope

Eventually, when we deal with namespaces properly, we will reimplement
something like yuujinchou.

Until then, we can make do with just local context.

We are also going to report at most one error for each top-level binding.
When we implement unification, we can revisit this.
-}
module Geolog.Elaboration where

import Control.Exception
import Data.Singletons
import Lens.Micro.Platform

import Geolog.Common
import Geolog.Core
import Geolog.Diagnostics
import Geolog.Diagnostics.Code qualified as C
import Geolog.Evaluation hiding (bind)
import Geolog.Notation (Ntn)
import Geolog.Notation qualified as N
import Geolog.Pretty hiding (bind)

type Ctx = Bwd (QName, Any TyV)

type CtxArg = (?ctx :: Ctx)

data DiagCtx = DiagCtx
  { diagCtxReporter :: Reporter
  , diagCtxFile :: File
  }

makeFields ''DiagCtx

type DiagCtxArg = (?diagCtx :: DiagCtx)

type Elab a = (DiagCtxArg, CtxArg, CtxLenArg, EnvArg) => a

data Glued s v (l :: Level) = G (s l) ~(v l)

type ElG = Glued ElS ElV
type TyG = Glued TyS TyV

gLiftTy :: LevelInclusion l l' -> TyG l -> TyG l'
gLiftTy li (G s v) = G (LiftTy s li) (VLiftTy v li)

data Syn (l :: Level) = Syn (ElG l) (TyV l)

data ElabException = GiveUp
  deriving (Show)

instance Exception ElabException

annot :: Ntn -> (QName, Ntn)
annot (N.Infix (N.Ident x _) (N.Keyword ":" _) n) = (x, n)
annot n = ("_", n)

bind :: forall l a. (SingI l) => Elab (QName -> TyV l -> (Elab a) -> a)
bind x va f =
  let
    s = sing @l
    vx = VNeu (FId ?ctxLen) SId
   in
    let
      ?env = ?env :> (Any s vx)
      ?ctx = ?ctx :> (x, Any s va)
      ?ctxLen = ?ctxLen + 1
     in
      f

report :: (DiagCtxArg) => Span -> C.Code -> IO a
report s c = do
  let n = Note (Just (SourceLoc (?diagCtx ^. file) s)) Nothing
  let d = Diagnostic c [n]
  reportIO (?diagCtx ^. reporter) d
  throw GiveUp

typ :: Elab (Sing l -> Ntn -> IO (TyG l))
typ s n = case n of
  N.Keyword "Query" _ -> case s of
    STheory -> pure $ G QueryU VQueryU
    SMeta -> pure $ gLiftTy TheoryInMeta $ G QueryU VQueryU
    _ -> report (N.span n) (C.WrongLevel "query universe" (fromSing s))
  N.Keyword "Theory" _ -> case s of
    SMeta -> pure $ G TheoryU VTheoryU
    _ -> report (N.span n) (C.WrongLevel "theory universe" (fromSing s))
  N.Infix n1 (N.Keyword "->" _) n2 -> case s of
    STheory -> do
      let (x, na) = annot n1
      G sa va <- typ SQuery na
      G sb _ <- bind x va $ typ STheory n2
      pure $ G (TheoryPi sa (Abs x sb)) (VTheoryPi va (Clo ?env x sb))
    SMeta -> do
      let (x, na) = annot n1
      G sa va <- typ SMeta na
      G sb _ <- bind x va $ typ SMeta n2
      pure $ G (MetaPi sa (Abs x sb)) (VMetaPi va (Clo ?env x sb))
    _ -> report (N.span n) (C.WrongLevel "pi types" (fromSing s))
  _ -> unimplemented

syn :: Elab (Ntn -> IO (Any Syn))
syn = \case
  _ -> unimplemented

chk :: Elab (Sing l -> TyV l -> Ntn -> IO (ElG l))
chk a = \case
  _ -> unimplemented
