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
import Geolog.Common
import Geolog.Core
import Geolog.Diagnostics
import Geolog.Diagnostics.Code qualified as C
import Geolog.Evaluation hiding (bind)
import Geolog.Notation (Ntn)
import Geolog.Notation qualified as N
import Geolog.Pretty hiding (bind)
import Lens.Micro.Platform
import Prelude hiding (lookup)

newtype Ctx = Ctx {ctxElts :: Bwd (QName, Any TyV)}

instance Lookup Ctx QName (BId, Any TyV) where
  lookup (Ctx elts) x = go elts 0
   where
    go BwdNil _ = Nothing
    go (es :> (x', va)) i
      | x == x' = Just (i, va)
      | otherwise = go es (i + 1)

type CtxArg = (?ctx :: Ctx)

data DiagCtx = DiagCtx
  { diagCtxReporter :: Reporter
  , diagCtxFile :: File
  }

makeFields ''DiagCtx

type DiagCtxArg = (?diagCtx :: DiagCtx)

type Elab a = (DiagCtxArg, CtxArg, CtxLenArg, EnvArg) => a

data Glued s v (l :: Level) = G {stx :: (s l), val :: ~(v l)}

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

bind :: Elab (Sing l -> QName -> TyV l -> (Elab a) -> a)
bind s x va f =
  let vx = VNeu (FId ?ctxLen) SId
   in let ?env = ?env :> (Any s vx)
          ?ctx = Ctx $ ctxElts ?ctx :> (x, Any s va)
          ?ctxLen = ?ctxLen + 1
       in f

report :: (DiagCtxArg) => Span -> C.Code -> IO a
report s c = do
  let n = Note (Just (SourceLoc (?diagCtx ^. file) s)) Nothing
  let d = Diagnostic c [n]
  reportIO (?diagCtx ^. reporter) d
  throw GiveUp

members :: Elab (Sing l -> [Ntn] -> IO [(QName, TyS l)])
members _ [] = pure []
members s (n : ns) = do
  let (x, n') = annot n
  G sa va <- typ s n'
  ((x, sa) :) <$> bind s x va (members s ns)

{- | How do we avoid getting trapped in an infinite loop with Code/El?

One option is to pass around another implicit variable about whether or not
we've tried a type yet. This seems hacky.

The thing is, some of the notations for type should really synthesize, morally
speaking.

We could add a new universe at the top which was unmentionable, so that `typ`
was really checking at this type...

Solution: we don't ever need to explicitly elaborate any meta-level types. They
show up as the types of top-level declarations, but never actually get parsed.
So therefore `typ` can just immediately call `chk` at a universe.
-}
gQueryCode :: TyG Query -> ElG Theory
gQueryCode (G sa va) = G (queryCode sa) (vQueryCode va)

gQueryEl :: ElG Theory -> TyG Query
gQueryEl (G sa va) = G (queryEl sa) (vQueryEl va)

gTheoryCode :: TyG Theory -> ElG Meta
gTheoryCode (G sa va) = G (theoryCode sa) (vTheoryCode va)

gTheoryEl :: ElG Meta -> TyG Theory
gTheoryEl (G sa va) = G (theoryEl sa) (vTheoryEl va)

typ :: Elab (Sing l -> Ntn -> IO (TyG l))
typ s n = case s of
  SQuery -> do
    ga <- chk STheory VQueryU n
    pure $ gQueryEl ga
  STheory -> do
    ga <- chk SMeta VTheoryU n
    pure $ gTheoryEl ga
  _ -> error "cannot elaborate type at this level"

theorySyn :: TyG Theory -> Any Syn
theorySyn ga = Any SMeta $ Syn (gTheoryCode ga) VTheoryU

gVar :: (EnvArg) => Sing l -> BId -> ElG l
gVar s i = G (Var i) (extractAt s $ elemAt ?env i)

gTheoryApp :: ElG Theory -> ElG Query -> ElG Theory
gTheoryApp (G sf vf) (G st vt) = G (TheoryApp sf st) (theoryApp vf vt)

gMetaApp :: ElG Meta -> ElG Meta -> ElG Meta
gMetaApp (G sf vf) (G st vt) = G (MetaApp sf st) (metaApp vf vt)

theoryCloApp :: Clo TyS Theory -> ElV Query -> TyV Theory
theoryCloApp (Clo env _ body) v = evalIn (env :> Any SQuery v) body

metaCloApp :: Clo TyS Meta -> ElV Meta -> TyV Meta
metaCloApp (Clo env _ body) v = evalIn (env :> Any SMeta v) body

syn :: Elab (Ntn -> IO (Any Syn))
syn n = case n of
  N.Ident x sp -> case lookup ?ctx x of
    Just (i, Any s va) -> pure $ Any s $ Syn (gVar s i) va
    Nothing -> report sp (C.NotInScope x)
  N.App n1 n2 -> do
    Any s (Syn gf vab) <- syn n1
    case s of
      STheory -> case vab of
        VTheoryPi va b -> do
          gt <- chk SQuery va n2
          pure $ Any s $ Syn (gTheoryApp gf gt) (theoryCloApp b (val gt))
        _ -> report (N.span n1) C.CannotApplyNonPi
      SMeta -> case vab of
        VMetaPi va b -> do
          gt <- chk SMeta va n2
          pure $ Any s $ Syn (gMetaApp gf gt) (metaCloApp b (val gt))
        _ -> report (N.span n1) C.CannotApplyNonPi
      _ -> report (N.span n1) C.CannotApplyNonPi
  N.Infix _ (N.Keyword "=>" _) _ -> report (N.span n) (C.MustChk "lambda syntax")
  N.Keyword "Query" _ -> pure $ theorySyn $ G QueryU VQueryU
  N.Infix n1 (N.Keyword "->" _) n2 -> do
    let (x, na) = annot n1
    G sa va <- typ SQuery na
    G sb _ <- bind SQuery x va $ typ STheory n2
    pure $ theorySyn (G (TheoryPi sa (Abs x sb)) (VTheoryPi va (Clo ?env x sb)))
  N.Tuple _ _ -> report (N.span n) (C.MustChk "tuple syntax")
  _ -> unimplemented

chk :: Elab (Sing l -> TyV l -> Ntn -> IO (ElG l))
chk s va n = case n of
  N.Tuple ns _ -> case va of
    VQueryU -> unimplemented
    VTheoryU -> unimplemented
    VRecord env fields -> unimplemented
    _ -> report (N.span n) (C.TupleFoundAtUnexpectedType (prtPrec precTop $ quote va))
  _ -> unimplemented

-- typ :: Elab (Sing l -> Ntn -> IO (TyG l))
-- typ s n = case n of
--   N.Keyword "Query" _ -> case s of
--     STheory -> pure $ G QueryU VQueryU
--     SMeta -> pure $ gLiftTy TheoryInMeta $ G QueryU VQueryU
--     _ -> report (N.span n) (C.WrongLevel "query universe" (fromSing s))
--   N.Keyword "Theory" _ -> case s of
--     SMeta -> pure $ G TheoryU VTheoryU
--     _ -> report (N.span n) (C.WrongLevel "theory universe" (fromSing s))
--   N.Infix n1 (N.Keyword "->" _) n2 -> case s of
--     STheory -> do
--       let (x, na) = annot n1
--       G sa va <- typ SQuery na
--       G sb _ <- bind SQuery x va $ typ STheory n2
--       pure $ G (TheoryPi sa (Abs x sb)) (VTheoryPi va (Clo ?env x sb))
--     SMeta -> do
--       let (x, na) = annot n1
--       G sa va <- typ SMeta na
--       G sb _ <- bind SMeta x va $ typ SMeta n2
--       pure $ G (MetaPi sa (Abs x sb)) (VMetaPi va (Clo ?env x sb))
--     _ -> report (N.span n) (C.WrongLevel "pi types" (fromSing s))
--   N.Tuple ns _ -> do
--     fs <- Fields <$> members s ns
--     pure $ G (Record fs) (VRecord ?env fs)
--   _ -> case s of
--     SSort -> QueryEl <$> chk s QueryU n
--     STheory -> TheoryEl <$> chk TheoryU n
--     _ ->
