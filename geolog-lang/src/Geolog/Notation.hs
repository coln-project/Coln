module Geolog.Notation where

import Prelude hiding (head, span)
import Data.Maybe (maybeToList)
import Geolog.Common
import Prettyprinter

-- We put the span last because in the parser we have utility methods
-- which take `Span -> Ntn`, and if the span is last, we can use currying
-- to our advantage here
data Ntn
  = App1 Ntn Ntn
  | App2 Ntn Ntn Ntn
  | Block Name (Maybe Ntn) (Fwd Ntn) Span
  | Decl Name Ntn Span
  | Ident Name Span
  | Field Name Span
  | Int Int Span
  | Error Span

head :: Ntn -> Doc ann
head (App1 _ _) = "App1"
head (App2 _ _ _) = "App2"
head (Block x _ _ _) = "Block" <+> pretty x
head (Decl x _ _) = "Decl" <+> pretty x
head (Ident x _) = "Ident" <+> pretty x
head (Field x _) = "Field" <+> pretty x
head (Int i _) = "Int" <+> pretty i
head (Error _) = "Error"

startPos :: Ntn -> Pos
startPos (App1 f _) = startPos f
startPos (App2 x _ _) = startPos x
startPos (Block _ _ _ s) = spanStart s
startPos (Decl _ _ s) = spanStart s
startPos (Ident _ s) = spanStart s
startPos (Field _ s) = spanStart s
startPos (Int _ s) = spanStart s
startPos (Error s) = spanStart s

endPos :: Ntn -> Pos
endPos (App1 _ x) = endPos x
endPos (App2 _ _ y) = endPos y
endPos (Block _ _ _ s) = spanEnd s
endPos (Decl _ _ s) = spanEnd s
endPos (Ident _ s) = spanEnd s
endPos (Field _ s) = spanEnd s
endPos (Int _ s) = spanEnd s
endPos (Error s) = spanEnd s

span :: Ntn -> Span
span n = Span (startPos n) (endPos n)

children :: Ntn -> [Ntn]
children (App1 f x) = [f, x]
children (App2 x op y) = [x, op, y]
children (Block _ mh xs _) = maybeToList mh ++ xs
children (Decl _ x _) = [x]
children (Ident _ _) = []
children (Field _ _) = []
children (Int _ _) = []
children (Error _) = []

instance Pretty Ntn where
  pretty n = if null cs then h else vsep [h, indent 2 $ vsep cs] where
    h = head n <+> "(" <> pretty (span n) <> ")"
    cs = map pretty (children n)
