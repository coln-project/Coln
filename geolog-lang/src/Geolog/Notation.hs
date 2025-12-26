module Geolog.Notation where

import Data.Maybe (maybeToList)
import Geolog.Common
import Prettyprinter
import Prelude hiding (head, span)

-- We put the span last because in the parser we have utility methods
-- which take `Span -> Ntn`, and if the span is last, we can use currying
-- to our advantage here
data Ntn
  = App Ntn Ntn
  | Infix Ntn Ntn Ntn
  | Block Name (Maybe Ntn) [Ntn] Span
  | Decl Name Ntn Span
  | Ident Name Span
  | Field Name Span
  | Int Int Span
  | Error Span

head :: Ntn -> Doc ann
head (App _ _) = "App"
head (Infix _ _ _) = "Infix"
head (Block x _ _ _) = "Block" <+> pretty x
head (Decl x _ _) = "Decl" <+> pretty x
head (Ident x _) = "Ident" <+> pretty x
head (Field x _) = "Field" <+> pretty x
head (Int i _) = "Int" <+> pretty i
head (Error _) = "Error"

startPos :: Ntn -> Pos
startPos (App f _) = startPos f
startPos (Infix x _ _) = startPos x
startPos (Block _ _ _ s) = spanStart s
startPos (Decl _ _ s) = spanStart s
startPos (Ident _ s) = spanStart s
startPos (Field _ s) = spanStart s
startPos (Int _ s) = spanStart s
startPos (Error s) = spanStart s

endPos :: Ntn -> Pos
endPos (App _ x) = endPos x
endPos (Infix _ _ y) = endPos y
endPos (Block _ _ _ s) = spanEnd s
endPos (Decl _ _ s) = spanEnd s
endPos (Ident _ s) = spanEnd s
endPos (Field _ s) = spanEnd s
endPos (Int _ s) = spanEnd s
endPos (Error s) = spanEnd s

span :: Ntn -> Span
span n = Span (startPos n) (endPos n)

children :: Ntn -> [Ntn]
children (App f x) = [f, x]
children (Infix x op y) = [x, op, y]
children (Block _ mh xs _) = maybeToList mh ++ xs
children (Decl _ x _) = [x]
children (Ident _ _) = []
children (Field _ _) = []
children (Int _ _) = []
children (Error _) = []

instance Pretty Ntn where
  pretty n = if null cs then h else vsep [h, indent 2 $ vsep cs]
   where
    h = head n <+> "(" <> pretty (span n) <> ")"
    cs = map pretty (children n)
