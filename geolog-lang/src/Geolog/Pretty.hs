module Geolog.Pretty where

import Geolog.Common
import Geolog.Core
import Geolog.Parser (Assoc (..), Prec (..), precLe)
import Prettyprinter

-- Implicit arguments for pretty printing
--------------------------------------------------------------------------------

type NamesArg = (?names :: Bwd QName)

bind :: (NamesArg) => QName -> ((NamesArg) => a) -> a
bind x f = let ?names = ?names :> x in f

type PrecArg = (?prec :: Prec)

type DoPretty a = (PrecArg) => (NamesArg) => a

-- Pretty printing
--------------------------------------------------------------------------------

class Prt a where
  prt :: DoPretty (a -> Doc ann)

prtPrec :: (Prt a) => (NamesArg) => Prec -> a -> Doc ann
prtPrec p x = let ?prec = p in prt x

instance Prt BId where
  prt (BId i) = go ?names i []
   where
    go (_ :> x) 0 prev = pretty x <> disamb
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

instance Prt (ElS l) where
  prt = \case
    Var i -> prt i
    QueryCode ty -> prt ty
    TheoryCode ty -> prt ty
    TheoryApp f t -> par precApp $ (prtPrec precApp f) <+> (prtPrec precApp t)
    TheoryLam (Abs x t) ->
      par precLam (pretty x <+> "=>" <+> bind x (prtPrec precLam t))
    MetaApp f t -> par precApp $ (prtPrec precApp f) <+> (prtPrec precArg t)
    MetaLam (Abs x t) ->
      par precLam (pretty x <+> "=>" <+> bind x (prtPrec precLam t))
    Proj t f -> par precApp $ (prtPrec precApp t) <+> "." <> pretty f
    Cons (Fields fs) ->
      list
        ["." <> pretty x <+> "=" <+> prtPrec precTop t | (x, t) <- fs]
    LiftEl t _ -> prt t

par :: (PrecArg) => Prec -> Doc ann -> Doc ann
par p s
  | precLe p ?prec == Just True = "(" <> s <> ")"
  | True = s

instance Prt (TyS l) where
  prt = \case
    QueryU -> "Query"
    QueryEl t -> prt t
    TheoryU -> "Theory"
    TheoryEl t -> prt t
    TheoryPi a (Abs x b) ->
      let annot = "(" <> pretty x <+> ":" <+> prtTop a <> ")"
       in par precLam (annot <+> "->" <+> bind x (prtPrec precLam b))
    MetaPi a (Abs x b) ->
      let annot = "(" <> pretty x <+> ":" <+> prtTop a <> ")"
       in par precLam (annot <+> "->" <+> bind x (prtPrec precLam b))
    Record (Fields fs) -> list $ go fs []
     where
      go :: DoPretty ([(QName, TyS l)] -> [Doc ann] -> [Doc ann])
      go [] ds = reverse ds
      go ((x, a) : rest) ds =
        let d = pretty x <+> ":" <+> prtPrec precTop a
         in bind x $ go rest (d : ds)
    LiftTy a _ -> prt a

prtTop :: (NamesArg, Prt a) => a -> Doc ann
prtTop x = let ?prec = precTop in prt x
