module FNotation.Pretty where

import Diagnostician
import FNotation.Config
import FNotation.Names
import FNotation.Trees
import Prettyprinter

data PrevPrec
  = LeftOf Prec
  | RightOf Prec
  | Top
  | Bot

tighter :: Prec -> PrevPrec -> Bool
tighter _ Top = True
tighter _ Bot = False
tighter (Prec b a) (LeftOf (Prec b' a'))
  | b > b' = True
  | b < b' = False
  | a == AssocL && a' == AssocL = True
  | otherwise = False
tighter (Prec b a) (RightOf (Prec b' a'))
  | b > b' = True
  | b < b' = False
  | a == AssocR && a' == AssocR = True
  | otherwise = False

looser :: Prec -> PrevPrec -> Bool
looser p p' = not $ tighter p p'

type ConfigArg = (?config :: ConfTable Prec)

prtTop :: (ConfigArg) => NtnGeneric a -> DDoc
prtTop = prt Top

precApp :: Prec
precApp = Prec 100 AssocL

par :: Bool -> DDoc -> DDoc
par True d = enclose "(" ")" d
par False d = d

prt :: (ConfigArg) => PrevPrec -> NtnGeneric a -> DDoc
prt p = \case
  App n ns ->
    par (looser precApp p) $
      prt (LeftOf precApp) n <+> hsep (prt (RightOf precApp) <$> ns)
  Infix l n r ->
    let mp' = case n of
          Ident x _ -> confTableLookup ?config x.last
          Keyword x _ -> confTableLookup ?config x.last
          _ -> Nothing
        p' = maybe (Prec 50 AssocL) id mp'
     in par (looser p' p) (prt (LeftOf p') l <+> prt Bot n <+> prt (RightOf p') r)
  Block x hd stmts _ ->
    vsep $
      [dpretty x <> maybe mempty ((" " <>) . prtTop) hd]
        ++ [indent 2 $ prtTop stmt | stmt <- stmts]
        ++ ["end"]
  Decl x n _ -> dpretty x <+> prtTop n
  Ident x _ -> dpretty x
  Keyword x _ -> dpretty x
  Field x _ -> "." <> dpretty x
  Tag x _ -> "'" <> dpretty x
  Int i _ -> pretty i
  String x _ -> "\"" <> pretty x <> "\""
  Tuple ns _ -> list $ prtTop <$> ns
  Error _ -> "<error>"

dprettyWithPrecs :: ConfTable Prec -> NtnGeneric a -> DDoc
dprettyWithPrecs config n = let ?config = config in prtTop n
