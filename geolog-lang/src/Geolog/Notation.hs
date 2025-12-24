module Geolog.Notation where

import Prelude hiding (head, span)
import Data.Maybe (maybeToList)
import Geolog.Common
import Prettyprinter

data Prec = LAssoc Int | NonAssoc Int | RAssoc Int
  deriving (Eq, Show)

-- We put the span last because in the parser we have utility methods
-- which take `Span -> Ntn`, and if the span is last, we can use currying
-- to our advantage here
data Ntn
  = App1 Ntn Ntn Span
  | App2 Ntn Ntn Ntn Span
  | Block Name (Maybe Ntn) (Fwd Ntn) Span
  | Decl Name Ntn Span
  | Ident Name Span
  | Field Name Span
  | Int Int Span
  | Error Span

head :: Ntn -> Doc ann
head (App1 _ _ _) = "App1"
head (App2 _ _ _ _) = "App2"
head (Block x _ _ _) = "Block" <+> pretty x
head (Decl x _ _) = "Decl" <+> pretty x
head (Ident x _) = "Ident" <+> pretty x
head (Field x _) = "Field" <+> pretty x
head (Int i _) = "Int" <+> pretty i
head (Error _) = "Error"

span :: Ntn -> Span
span (App1 _ _ s) = s
span (App2 _ _ _ s) = s
span (Block _ _ _ s) = s
span (Decl _ _ s) = s
span (Ident _ s) = s
span (Field _ s) = s
span (Int _ s) = s
span (Error s) = s

children :: Ntn -> [Ntn]
children (App1 f x _) = [f, x]
children (App2 op x y _) = [x, op, y]
children (Block _ mh xs _) = maybeToList mh ++ xs
children (Decl _ x _) = [x]
children (Ident _ _) = []
children (Field _ _) = []
children (Int _ _) = []
children (Error _) = []

instance Pretty Ntn where
  pretty n = vsep [
    head n <+> "(" <> pretty (span n) <> ")",
    indent 2 $ vsep $ map pretty (children n)]
