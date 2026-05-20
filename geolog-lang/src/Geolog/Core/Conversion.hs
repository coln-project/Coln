module Geolog.Core.Conversion where

import Control.Applicative ((<|>))
import Control.Monad (forM_, unless)
import Data.Maybe (fromMaybe)
import Data.Vector.Strict qualified as Vec
import Prettyprinter ((<+>))

import Geolog.Common
import Geolog.Core.Params
import Geolog.Core.Value qualified as V
import Geolog.Core.Value qualified as N (Neutral(..))
import Geolog.Core.Value qualified as DN (DecodedNeutral(..))

data DefEqCheckError
  = UnequalTys CtxShape (V.Ty N) (V.Ty N) (Maybe DDoc)
  | UnequalEls CtxShape (V.El N) (V.El N) (Maybe DDoc)

type DefEqM a = Either DefEqCheckError a

throwUnequalTys :: CtxShape -> V.Ty N -> V.Ty N -> Maybe DDoc -> DefEqM ()
throwUnequalTys cs a a' e =
  Left (UnequalTys cs a a' e)

throwUnequalEls :: CtxShape -> V.El N -> V.El N -> Maybe DDoc -> DefEqM ()
throwUnequalEls cs v v' e =
  Left (UnequalEls cs v v' e)

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

-- XXX should the rest of these be nested cases instead of parallel cases?
--     more duplication but guards against forgetting to update.

-- XXX should universe subsumption be deep?  should we be doing substitutability not equality?
instance DefEq (V.Ty N) where
  defEq cs a a' = case (a, a') of
    (V.U u, V.U u') | u == u' -> pure ()
    (V.Decode n, V.Decode n') -> defEq cs n n'
    (V.Function f, V.Function f') -> do
      unless (f.variant == f'.variant) $
        throwUnequalTys cs a a' $
          Just $
            "different function variants:" <+> pretty f.variant <+> "and" <+> pretty f'.variant
      defEq cs f.dom f'.dom
      defEqClo cs f.dom f.cod f'.cod
    (V.BuiltinTy b, V.BuiltinTy b') ->
      unless (b == b') $ throwUnequalTys cs a a' $ Just "unequal builtin types"
    _ -> throwUnequalTys cs a a' Nothing

-- XXX recursive neutrals are not valid except for head, spine.  Pretty printing should only use these, but still kind of ...
--     To remedy this, maybe it would work to reapply the spine to the head, but that only works for globals ...
instance DefEq V.Neutral where
  defEq cs n n' = case (n.spine, n'.spine) of
    (V.Id, V.Id) -> case (n.head, n'.head) of
      (V.LocalVar i, V.LocalVar i') | i == i' -> pure ()
      (V.GlobalVar x _, V.GlobalVar x' _) | x == x' -> pure ()
      _ -> throwUnequalEls cs (V.Neu n) (V.Neu n') Nothing
    (V.App sq v, V.App sq' v') -> do
      defEq cs (n { N.spine = sq }) (n' { N.spine = sq' })
      defEq cs v v'
    (V.Proj sq x, V.Proj sq' x') -> do
      defEq cs (n { N.spine = sq }) (n' { N.spine = sq' })
      unless (x == x') $
        throwUnequalEls cs (V.Neu n) (V.Neu n') Nothing
    _ -> throwUnequalEls cs (V.Neu n) (V.Neu n') Nothing

codeOfNeu :: V.DecodedNeutral -> V.Neutral
codeOfNeu n = V.Neutral
  { N.head = n.head
  , N.spine = n.spine
  , N.ty = V.U n.universe
  , N.expansion = V.NotApplicable
  , N.description = panic "tried to use description of reverse-engineered neutral"
  }

instance DefEq V.DecodedNeutral where
  defEq cs n n' = defEq cs (codeOfNeu n) (codeOfNeu n')

canon :: V.El N -> V.El N
canon v@(V.Neu n) = case V.behavior n.ty of
  V.LikeRecord _ -> V.Cons (V.unwrap n.expansion)
  -- XXX is it okay to use LNil here?
  V.LikeFunction f -> V.Lam f.dom $ V.Clo "x" V.LNil $ \w -> V.app v (elemAt w (BId 0))
  _ -> v
canon v = v

instance DefEq (V.El N) where
  defEq cs v v' = case (v, v') of
    -- XXX is this safe with descriptive case/behavesAs?
    -- probably isn't, but it's useful enough to be worth *making* safe.
    -- (V.Neu n, V.Neu n') -> defEq cs n n' -- shortcut eta expansion
    _ -> case (canon v, canon v') of
      (V.Neu n, V.Neu n') -> defEq cs n n'
      (V.Code a, V.Code a') -> defEq cs a a'
      (V.Lam a c, V.Lam _ c') -> defEqClo cs a c c'
      (V.Cons d, V.Cons d') ->
        forM_ (Vec.zip d.values d'.values) (uncurry (defEq cs))
      (V.Lit l, V.Lit l') ->
        unless (l == l') $ throwUnequalEls cs v v' $ Just "unequal literals"
      _ -> throwUnequalEls cs v v' Nothing

