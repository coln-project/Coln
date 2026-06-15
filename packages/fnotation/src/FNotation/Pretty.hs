module FNotation.Pretty where

import Data.Maybe (fromMaybe)

import Diagnostician
import FNotation.Config
import FNotation.Kinds (Kind)
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

type ConfigArg = (?config :: ConfTable Prec, ?lconfig :: ConfTable Kind)

tryImmediate :: (ConfigArg) => NtnGeneric a -> Maybe DDoc
tryImmediate (Ident x _) = return $ dprettyWithKinds ?lconfig x
tryImmediate (Juxt n (Field x _)) = do
  i <- tryImmediate n
  return $ i <> "." <> dprettyWithKinds ?lconfig x
tryImmediate _ = Nothing

prtTop :: (ConfigArg) => NtnGeneric a -> DDoc
prtTop = prt Top

precApp :: Prec
precApp = Prec 100 AssocL

par :: Bool -> DDoc -> DDoc
par True d = enclose "(" ")" d
par False d = d

prt :: (ConfigArg) => PrevPrec -> NtnGeneric a -> DDoc
prt p = \case
  Juxt n n' -> fromMaybe
    (par (looser precApp p) $
      prt (LeftOf precApp) n <+> prt (RightOf precApp) n')
    (tryImmediate (Juxt n n'))
  Infix l n r ->
    let mp' = case n of
          Ident x _ -> confTableLookup ?config x.last
          Keyword x _ -> confTableLookup ?config x.last
          _ -> Nothing
        p' = fromMaybe (Prec 50 AssocL) mp'
     in par (looser p' p) (prt (LeftOf p') l <+> prt Bot n <+> prt (RightOf p') r)
  Block x hd stmts _ ->
    vsep $
      [dpretty x <> maybe mempty ((" " <>) . prtTop) hd]
        ++ [indent 2 $ prtTop stmt | stmt <- stmts]
        ++ ["end"]
  MDecl ms x n _ -> hsep (dpretty <$> (ms ++ [x])) <+> prtTop n
  Ident x _ | Bot <- p -> dprettyOpWithKinds ?lconfig x
  Ident x _ -> dprettyWithKinds ?lconfig x
  Keyword x _ -> dpretty x
  Field x _ -> "." <> dprettyWithKinds ?lconfig x
  Tag x _ -> "'" <> dprettyWithKinds ?lconfig x
  Int i _ -> pretty i
  String x _ -> "\"" <> pretty x <> "\""
  Tuple ns _ -> list $ prtTop <$> ns
  Error _ -> "<error>"

dprettyWithConfigs :: ConfTable Prec -> ConfTable Kind -> NtnGeneric a -> DDoc
dprettyWithConfigs config lconfig n = let ?config = config; ?lconfig = lconfig in prtTop n
