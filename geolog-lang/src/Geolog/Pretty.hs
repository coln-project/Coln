module Geolog.Pretty where

import Diagnostician
import FNotation (Assoc (..), Name, Prec (..), precLe)
import Geolog.Common
import Geolog.Core
import Prettyprinter

-- Pretty printing
--------------------------------------------------------------------------------

type Names = Bwd Name

class Prt a where
  prt :: Prec -> Names -> a -> DDoc

instance Prt BId where
  prt _ xs (BId i) = go xs i []
   where
    go (_ :> x) 0 prev = dpretty x <> disamb
     where
      nx = length $ filter (== x) prev
      disamb = if nx > 0 then "^" <> pretty nx else ""
    go (xs' :> x) n prev = go xs' (n - 1) (x : prev)
    go BwdNil _ _ = error $ "name " ++ show i ++ " not bound. ?names = " ++ (show $ toList xs)

precApp :: Prec
precApp = Prec 100 AssocL

precArg :: Prec
precArg = Prec 101 AssocL

precLam :: Prec
precLam = Prec 20 AssocL

precTop :: Prec
precTop = Prec 0 AssocNon

instance Prt (ElS e) where
  prt p xs = \case
    LocalVar i -> prt p xs i
    GlobalVar x -> dpretty x
    Code ty -> prt p xs ty
    App f t -> par p precApp (\_ -> prt precApp xs f <+> (prt precApp xs t))
    Lam _ (Abs x t) ->
      par p precLam (\_ -> dpretty x <+> "=>" <+> (prt precLam (xs :> x) t))
    Lam _ (AbsConst t) ->
      par p precLam (\_ -> "_" <+> "=>" <+> prt precLam xs t)
    Proj t f -> par p precApp (\_ -> prt precApp xs t <+> "." <> dpretty f)
    Cons (Fields ys ts) ->
      list
        ["." <> dpretty y <+> "=" <+> prtTop xs t | (y, t) <- zip ys ts]
    Lit l -> pretty l

par :: Prec -> Prec -> (Prec -> DDoc) -> DDoc
par p p' s
  | precLe p' p == Just True = "(" <> s precTop <> ")"
  | otherwise = s p

piVariantArr :: PiVariant -> DDoc
piVariantArr = \case
  PrimTheory -> "~>"
  QueryTheory -> "->"
  TheoryTop -> "->"

instance Prt (TyS e) where
  prt p xs = \case
    U u -> pretty $ decodesInto u
    Decode _ t -> prt p xs t
    Pi pv a (Abs x b) ->
      let annot = "(" <> dpretty x <+> ":" <+> prtTop xs a <> ")"
       in par p precLam (\_ -> annot <+> piVariantArr pv <+> prt precLam (xs :> x) b)
    Pi pv a (AbsConst b) ->
      par p precLam (\_ -> prt p xs a <+> piVariantArr pv <+> prt precLam xs b)
    Record _ names te -> list $ go xs names te []
     where
      go :: Names -> [Name] -> TeleS K -> [DDoc] -> [DDoc]
      go _ [] TSNil ds = reverse ds
      go xs (x : xs') (TSCons a te) ds =
        let d = dpretty x <+> ":" <+> prtTop xs a
         in go (xs :> x) xs' te (d : ds)
      go _ _ _ _ = panic "names and telescope should be same length"
    BuiltinTy a -> pretty a

prtTop :: (Prt a) => Names -> a -> DDoc
prtTop = prt precTop
