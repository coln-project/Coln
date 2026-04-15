module Geolog.Pretty where

import Data.String (fromString)
import Data.Text qualified as T
import Diagnostician
import FNotation qualified as N
import FNotation.Names
import Geolog.Common
import Geolog.Core
import Geolog.Notation

-- Pretty printing
--------------------------------------------------------------------------------

type Names = Bwd Name

class Delab a where
  delab :: Names -> a -> N.Ntn0

instance Delab BId where
  delab xs (BId i) = go xs i []
   where
    go (_ :> x) 0 prev = N.Ident (x{last = x.last <> disamb}) ()
     where
      nx = length $ filter (== x) prev
      disamb = if nx > 0 then "^" <> T.pack (show nx) else ""
    go (xs' :> x) n prev = go xs' (n - 1) (x : prev)
    go BwdNil _ _ = error $ "name " ++ show i ++ " not bound. ?names = " ++ (show $ toList xs)

instance Delab (ElS e) where
  delab xs = \case
    LocalVar i -> delab xs i
    GlobalVar x -> N.Ident x ()
    Code ty -> delab xs ty
    App f t -> N.App (delab xs f) [delab xs t]
    Lam _ (Abs x t) ->
      N.Infix (N.Ident x ()) (N.Keyword "=>" ()) (delab (xs :> x) t)
    Lam _ (AbsConst t) ->
      N.Infix (N.Ident "_" ()) (N.Keyword "=>" ()) (delab xs t)
    Proj t f -> N.App (delab xs t) [N.Field f ()]
    Cons (Fields ys ts) ->
      N.Tuple [field y t | (y, t) <- zip ys ts] ()
     where
      field y t = N.Infix (N.Ident y ()) (N.Keyword ":" ()) (delab xs t)
    Lit (LitInt i) -> N.Int i ()
    Lit (LitString s) -> N.String s ()
    Init a -> N.App (N.Keyword "init" ()) [delab xs a]
    Pure t -> N.App (N.Keyword "pure" ()) [delab xs t]
    Use t -> N.App (delab xs t) [N.Field "use" ()]

bindingModeArr :: BindingMode -> Name
bindingModeArr BInductive = "*->"
bindingModeArr BConjunctive = "->"

piVariantArr :: PiVariant -> Name
piVariantArr = bindingModeArr . bindingMode

nbinding :: Name -> N.Ntn0 -> N.Ntn0
nbinding x n = N.Infix (N.Ident x ()) (N.Keyword ":" ()) n

instance Delab (TyS e) where
  delab xs = \case
    U u -> N.Keyword (fromString $ show $ decodesInto u) ()
    Decode t -> delab xs t
    Pi pv a (Abs x b) ->
      N.Infix
        (nbinding x (delab xs a))
        (N.Keyword (piVariantArr pv) ())
        (delab (xs :> x) b)
    Pi pv a (AbsConst b) ->
      N.Infix (delab xs a) (N.Keyword (piVariantArr pv) ()) (delab xs b)
    Record _ ys te -> N.Block "sig" Nothing (go xs ys te) ()
     where
      go _ [] TSNil = []
      go xs' (y : ys') (TSCons a te') =
        nbinding y (delab xs' a) : go (xs' :> y) ys' te'
      go _ _ _ = error "mismatching length for names and telescope"
    Eq _ t0 t1 -> N.Infix (delab xs t0) (N.Keyword "=" ()) (delab xs t1)
    BuiltinTy a -> N.Keyword (fromString $ show a) ()
    Inductive a -> N.App (N.Keyword "Inductive" ()) [delab xs a]

class DPrettyWithNames a where
  dprettyWithNames :: Names -> a -> DDoc

instance DPrettyWithNames BId where
  dprettyWithNames xs t = N.dprettyWithPrecs parseConfig $ delab xs t

instance DPrettyWithNames (ElS e) where
  dprettyWithNames xs t = N.dprettyWithPrecs parseConfig $ delab xs t

instance DPrettyWithNames (TyS e) where
  dprettyWithNames xs t = N.dprettyWithPrecs parseConfig $ delab xs t
