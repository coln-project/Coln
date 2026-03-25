module Geolog.Pretty where

import Geolog.Common
import Geolog.Core
import FNotation (Name, Prec (..), Assoc (..), precLe)
import Diagnostician
import Prettyprinter

-- Implicit arguments for pretty printing
--------------------------------------------------------------------------------

type NamesArg = (?names :: Bwd Name)

bindName :: (NamesArg) => Name -> ((NamesArg) => a) -> a
bindName x f = let ?names = ?names :> x in f

type PrecArg = (?prec :: Prec)

type DoPretty a = (PrecArg) => (NamesArg) => a

-- Pretty printing
--------------------------------------------------------------------------------

class Prt a where
  prt :: DoPretty (a -> DDoc)

prtPrec :: (Prt a) => (NamesArg) => Prec -> a -> DDoc
prtPrec p x = let ?prec = p in prt x

instance Prt BId where
  prt (BId i) = go ?names i []
    where
      go (_ :> x) 0 prev = dpretty x <> disamb
        where
          nx = length $ filter (== x) prev
          disamb = if nx > 0 then "^" <> pretty nx else ""
      go (xs :> x) n prev = go xs (n - 1) (x : prev)
      go BwdNil _ _ = error $ "name " ++ show i ++ " not bound. ?names = " ++ (show $ toList ?names)

precApp :: Prec
precApp = Prec 100 AssocL

precArg :: Prec
precArg = Prec 101 AssocL

precLam :: Prec
precLam = Prec 20 AssocL

precTop :: Prec
precTop = Prec 0 AssocNon

instance Prt (ElS e) where
  prt = \case
    Var i -> prt i
    GlobalVar x -> dpretty x
    Code ty -> prt ty
    App f t -> par precApp $ (prtPrec precApp f) <+> (prtPrec precApp t)
    Lam _ (Abs x t) ->
      par precLam (dpretty x <+> "=>" <+> bindName x (prtPrec precLam t))
    Lam _ (AbsConst t) ->
      par precLam ("_" <+> "=>" <+> prtPrec precLam t)
    Proj t f -> par precApp $ (prtPrec precApp t) <+> "." <> dpretty f
    Cons (Fields xs ts) ->
      list
        ["." <> dpretty x <+> "=" <+> prtPrec precTop t | (x, t) <- zip xs ts]
    Lit l -> pretty l

par :: (PrecArg) => Prec -> ((PrecArg) => DDoc) -> DDoc
par p s
  | precLe p ?prec == Just True = "(" <> let ?prec = precTop in s <> ")"
  | True = s

piVariantArr :: PiVariant -> DDoc
piVariantArr = \case
  PrimTheory -> "~>"
  QueryTheory -> "->"
  TheoryTop -> "->"

instance Prt (TyS e) where
  prt = \case
    U u -> pretty $ decodesInto u
    Decode _ t -> prt t
    Pi pv a (Abs x b) ->
      let annot = "(" <> dpretty x <+> ":" <+> prtTop a <> ")"
       in par precLam (annot <+> piVariantArr pv <+> bindName x (prtPrec precLam b))
    Pi pv a (AbsConst b) ->
      par precLam (prt a <+> piVariantArr pv <+> prtPrec precLam b)
    Record _ xs as -> list $ go (zip xs as) []
      where
        go :: DoPretty ([(Name, TyS e')] -> [DDoc] -> [DDoc])
        go [] ds = reverse ds
        go ((x, a) : rest) ds =
          let d = dpretty x <+> ":" <+> prtPrec precTop a
           in bindName x $ go rest (d : ds)
    BuiltinTy a -> pretty a

prtTop :: (NamesArg, Prt a) => a -> DDoc
prtTop x = let ?prec = precTop in prt x
