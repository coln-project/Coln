module Coln.Core.Print where

import Data.String (fromString)
import Data.Text qualified as T
import Diagnostician
import FNotation qualified as N
import FNotation.Names
import Coln.Common
import Coln.Core.Params
import Coln.Core.Syntax
import Coln.Core.Readback
import Coln.Notation

-- Pretty printing
--------------------------------------------------------------------------------

type Names = Bwd Name

class ToNotation a where
  toNotation :: Names -> a -> N.Ntn0

instance ToNotation BId where
  toNotation xs (BId i) = go xs i []
   where
    go (_ :> x) 0 prev = N.Ident (x{last = x.last <> disamb}) ()
     where
      nx = length $ filter (== x) prev
      disamb = if nx > 0 then "^" <> T.pack (show nx) else ""
    go (xs' :> x) n prev = go xs' (n - 1) (x : prev)
    go BwdNil _ _ = error $ "name " ++ show i ++ " not bound. ?names = " ++ (show $ toList xs)

instance ToNotation (El e) where
  toNotation xs = \case
    LocalVar i -> toNotation xs i
    GlobalVar x _ -> N.Ident x ()
    Code ty -> toNotation xs ty
    App f t -> N.Juxt (toNotation xs f) (toNotation xs t)
    Lam _ (Abs x t) ->
      N.Infix (N.Ident x ()) (N.Keyword "=>" ()) (toNotation (xs :> x) t)
    Lam _ (AbsConst t) ->
      N.Infix (N.Ident "_" ()) (N.Keyword "=>" ()) (toNotation xs t)
    Proj t f -> N.Juxt (toNotation xs t) (N.Field f ())
    Cons d ->
      N.Tuple [field y t | (y, t) <- toList d] ()
     where
      field y t = N.Infix (N.Ident y ()) (N.Keyword ":" ()) (toNotation xs t)
    Lit (LitInt i) -> N.Int i ()
    Lit (LitString s) -> N.String s ()
    Is t -> toNotation xs t -- invisible

nbinding :: Name -> N.Ntn0 -> N.Ntn0
nbinding x n = N.Infix (N.Ident x ()) (N.Keyword ":" ()) n

instance ToNotation (Ty e) where
  toNotation xs = \case
    U u -> N.Keyword (fromString $ show $ decodesInto u) ()
    Decode t -> toNotation xs t
    Function f -> case f.cod of 
      Abs x b -> N.Infix
        (nbinding x (toNotation xs f.dom))
        (N.Keyword "->" ())
        (toNotation (xs :> x) b)
      AbsConst b -> N.Infix
        (toNotation xs f.dom)
        (N.Keyword "->" ())
        (toNotation xs b)
    Record r -> N.Block "sig" Nothing (go xs $ toList r.fieldTypes) ()
     where
      go _ [] = []
      go xs' ((y, a) : pairs') =
        nbinding y (toNotation xs' a) : go (xs' :> y) pairs'
    Eq eq -> N.Infix
      (toNotation xs eq.lhs)
      (N.Keyword "=" ())
      (toNotation xs eq.rhs)
    BuiltinTy a -> N.Keyword (fromString $ show a) ()

class DPrettyWithNames a where
  dprettyWithNames :: Names -> a -> DDoc

instance DPrettyWithNames BId where
  dprettyWithNames xs t = N.dprettyWithConfigs parseConfig lexConfig $ toNotation xs t

instance DPrettyWithNames (El e) where
  dprettyWithNames xs t = N.dprettyWithConfigs parseConfig lexConfig $ toNotation xs t

instance DPrettyWithNames (Ty e) where
  dprettyWithNames xs t = N.dprettyWithConfigs parseConfig lexConfig $ toNotation xs t

class HasShape a where
  shape :: a -> CtxShape

instance HasShape CtxShape where
  shape = id

prtIn :: (HasShape c, Readback a b, DPrettyWithNames b) => c -> a -> DDoc
prtIn c v = do
  let cs = shape c
  dprettyWithNames cs.names $ readb cs.len v
