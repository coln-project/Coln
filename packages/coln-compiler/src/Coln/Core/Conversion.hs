-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Core.Conversion where

import Coln.Common
import Coln.Core.Params
import Coln.Core.Print (prtIn)
import Coln.Core.Value qualified as BN (BareNeutral (..))
import Coln.Core.Value qualified as V
import Control.Applicative ((<|>))
import Control.Monad (forM_, unless, zipWithM_)
import Data.Foldable qualified as F
import Data.Maybe (fromMaybe)
import Data.Vector.Strict qualified as Vec

data DefEqCheckError
  = UnequalTys CtxShape (V.Ty N) (V.Ty N) (Maybe DDoc)
  | UnequalEls CtxShape (V.El N) (V.El N) (Maybe DDoc)
  | UnequalNeus CtxShape V.BareNeutral V.BareNeutral (Maybe DDoc)

type DefEqM a = Either DefEqCheckError a

throwUnequalTys :: CtxShape -> V.Ty N -> V.Ty N -> Maybe DDoc -> DefEqM ()
throwUnequalTys cs a a' e =
  Left (UnequalTys cs a a' e)

throwUnequalEls :: CtxShape -> V.El N -> V.El N -> Maybe DDoc -> DefEqM ()
throwUnequalEls cs v v' e =
  Left (UnequalEls cs v v' e)

throwUnequalNeus :: CtxShape -> V.BareNeutral -> V.BareNeutral -> Maybe DDoc -> DefEqM ()
throwUnequalNeus cs v v' e =
  Left (UnequalNeus cs v v' e)

instance DPretty DefEqCheckError where
  dpretty (UnequalTys cs a a' reason) = "types" <+> prtIn cs a <+> "and" <+> prtIn cs a' <+> "are not equal" <+> maybe "" ("because" <+>) reason
  dpretty (UnequalEls cs a a' reason) = "values" <+> prtIn cs a <+> "and" <+> prtIn cs a' <+> "are not equal" <+> maybe "" ("because" <+>) reason
  dpretty (UnequalNeus cs a a' reason) = prtIn cs a <+> "and" <+> prtIn cs a' <+> "are not equal" <+> maybe "" ("because" <+>) reason

class DefEq a where
  defEq :: CtxShape -> a -> a -> DefEqM ()

cloName :: V.Clo a c -> Maybe Name
cloName (V.Clo n _ _) = Just n
cloName (V.CloConst _) = Nothing

-- XXX freshen the variable name?
defEqClo :: (DefEq (a N)) => CtxShape -> V.Ty N -> V.Clo a N -> V.Clo a N -> DefEqM ()
defEqClo cs a c0 c1 = do
  let n = fromMaybe "x" (cloName c0 <|> cloName c1)
  let cs' = CtxShape (cs.len + 1) (cs.names :> n)
  let v = V.local (FId cs.len) a
  defEq cs' (V.appClo c0 v) (V.appClo c1 v)

instance DefEq (V.Ty N) where
  defEq cs a a' = case a of
    V.U u -> case a' of
      V.U u' | u == u' -> pure ()
      _ -> throwUnequalTys cs a a' Nothing
    V.Decode n -> case a' of
      V.Decode n' -> defEq cs n n'
      _ -> throwUnequalTys cs a a' Nothing
    V.Function f -> case a' of
      V.Function f' -> do
        unless (f.variant == f'.variant) $
          throwUnequalTys cs a a' $
            Just $
              "different function variants:" <+> pretty f.variant <+> "and" <+> pretty f'.variant
        defEq cs f.dom f'.dom
        defEqClo cs f.dom f.cod f'.cod
      _ -> throwUnequalTys cs a a' Nothing
    V.Eq et -> case a' of
      V.Eq et' -> do
        defEq cs et.at et'.at
        defEq cs et.lhs et'.lhs
        defEq cs et.rhs et'.rhs
      _ -> throwUnequalTys cs a a' Nothing
    V.BuiltinTy b -> case a' of
      V.BuiltinTy b' ->
        unless (b == b') $ throwUnequalTys cs a a' $ Just "unequal builtin types"
      _ -> throwUnequalTys cs a a' Nothing
    V.EltOf x vs -> case a' of
      V.EltOf x' vs' -> do
        unless (x == x') $ throwUnequalTys cs a a' $ Just "unequal table names"
        zipWithM_ (defEq cs) (F.toList vs) (F.toList vs')
      _ -> throwUnequalTys cs a a' Nothing

instance DefEq V.Head where
  defEq cs h h' = case h of
    V.LocalVar i -> case h' of
      V.LocalVar i' | i == i' -> pure ()
      _ -> throwUnequalNeus cs (V.BareNeutral h V.Id) (V.BareNeutral h' V.Id) Nothing
    V.GlobalVar x _ -> case h' of
      V.GlobalVar x' _ | x == x' -> pure ()
      _ -> throwUnequalNeus cs (V.BareNeutral h V.Id) (V.BareNeutral h' V.Id) Nothing

instance DefEq V.BareNeutral where
  defEq cs n n' = case n.spine of
    V.Id -> case n'.spine of
      V.Id -> defEq cs n.head n'.head
      _ -> throwUnequalNeus cs n n' Nothing
    V.App sq v -> case n'.spine of
      V.App sq' v' -> do
        defEq cs (n{BN.spine = sq}) (n'{BN.spine = sq'})
        defEq cs v v'
      _ -> throwUnequalNeus cs n n' Nothing
    V.Proj sq x -> case n'.spine of
      V.Proj sq' x' -> do
        defEq cs (n{BN.spine = sq}) (n'{BN.spine = sq'})
        unless (x == x') $ throwUnequalNeus cs n n' Nothing
      _ -> throwUnequalNeus cs n n' Nothing

instance DefEq V.DecodedNeutral where
  defEq cs n n' = defEq cs (V.toBare n) (V.toBare n')

instance DefEq V.Neutral where
  defEq cs n n' = defEq cs (V.toBare n) (V.toBare n')

canon :: V.El N -> V.El N
canon v@(V.Neu n) = case V.behavior n.ty of
  V.LikeRecord _ -> V.Cons (V.unwrap n.expansion)
  -- XXX is it okay to use LNil here?
  V.LikeFunction f -> V.Lam f.dom $ V.Clo "x" V.LNil $ \w -> V.app v (elemAt w (BId 0))
  _ -> v
canon v = v

instance DefEq (V.El N) where
  -- XXX shortcut try without eta-expansion first
  defEq cs v v' = case canon v of
    V.Neu n -> case canon v' of
      V.Neu n' -> defEq cs n n'
      _ -> throwUnequalEls cs v v' Nothing
    V.Code a -> case canon v' of
      V.Code a' -> defEq cs a a'
      _ -> throwUnequalEls cs v v' Nothing
    V.Lam a c -> case canon v' of
      V.Lam _ c' -> defEqClo cs a c c'
      _ -> throwUnequalEls cs v v' Nothing
    V.Cons d -> case canon v' of
      V.Cons d' -> forM_ (Vec.zip d.values d'.values) (uncurry (defEq cs))
      _ -> throwUnequalEls cs v v' Nothing
    V.Lit l -> case canon v' of
      V.Lit l' -> unless (l == l') $ throwUnequalEls cs v v' $ Just "unequal literals"
      _ -> throwUnequalEls cs v v' Nothing
    V.Lookup x vs -> case canon v' of
      V.Lookup x' vs' -> do
        unless (x == x') $ throwUnequalEls cs v v' $ Just "unequal table names"
        zipWithM_ (defEq cs) (F.toList vs) (F.toList vs') -- XXX check heads?
      _ -> throwUnequalEls cs v v' Nothing
