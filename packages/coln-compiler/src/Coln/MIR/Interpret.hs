module Coln.MIR.Interpret where

-- Interpret Core syntax into MIR values
import Coln.Common

import Coln.MIR.Value qualified as MV
import Coln.Core.Syntax qualified as CS
import Coln.Core.Globals
import Coln.Core.Params
import Coln.Core.Memoed (Memoed (..))

-- No FunctionalDependency here, because we interpret syntax into different
-- parts!
class Interp a b where
  interp :: Globals -> MV.Locals -> a -> b

-- We need to plumb through more information about variants in the syntax.
--
-- Universe for Code
-- FunctionVariant for Lam/App
-- Level for Cons/Proj

-- This probably needs to also go through values.

-- Alternatives:
-- - Coercion from TopLam and TheoryCode downward
--
-- Arguably, the "right" way to do this is to plumb through that information.
-- Or rather... looking at the SOGAT, this information is *not* part of the syntax
-- for function/records, but *is* for universe operations.
-- Which implies that we should plumb through for Code/Decode, but not for
-- Lam/App/Cons/Proj...

-- I guess we are, in a sense, *always* in checking mode?
-- Let's try coercion

-- Another option: bidirectional
-- Another option: GADT

interpAbs :: (Interp (f c) b) => Globals -> MV.Locals -> CS.Abs f c -> MV.Clo b
interpAbs g l (CS.Abs x body) = MV.Clo x (\v -> interp g (l :> v) body)
interpAbs g l (CS.AbsConst t) = MV.CloConst (interp g l t)

appClo :: MV.Clo b -> MV.Model -> b
appClo (MV.Clo _ f) v = f v
appClo (MV.CloConst v) _ = v

app :: MV.Top -> MV.Top -> MV.Top
app f v  = case v of
  MV.Model v -> case f of
    MV.TopLam f -> appClo f v
    MV.Model (MV.Lam f) -> MV.Model $ appClo f v
    _ -> panic "expected lambda"
  _ -> panic "cannot apply function to non-model value"

instance Interp (CS.El c) MV.Top where
  interp g l = \case
    CS.LocalVar i -> MV.Model $ elemAt l i
    CS.GlobalVar x _ -> do
      let def = elemAt g.definitions x
      interp g l def.body.stx
    CS.Code u a -> case u of
      (PropU; SetU) -> MV.Model $ MV.All $ interp g l a
      TheoryU -> MV.TheoryCode $ interp g l a
    CS.Lam _ abs -> MV.TopLam $ interpAbs g l abs
    CS.App t0 t1 -> app (interp g l t0) (interp g l t1)
    CS.Cons fields -> 
      

instance Interp (CS.Ty c) MV.Ty where
  interp = undefined

instance Interp (CS.Ty c) MV.Theory where
  interp = undefined
