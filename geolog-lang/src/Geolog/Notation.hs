module Geolog.Notation where

import Data.Maybe (maybeToList)
import Geolog.Common
import Prettyprinter
import Prelude hiding (head, span)

-- Notation data structure
--------------------------------------------------------------------------------

{- | Notation is the output of parsing.

Each notation is associated with a span. Most variants store that span; `App`
and `Infix` don't because the span can be inferred by looking at the left and
right children.

In the variants which take a span, we put the span last because in the parser
we have utility methods which take `Span -> Ntn`, and if the span is last, we
can use currying to our advantage here.
-}
data Ntn
  = App Ntn Ntn
  | Infix Ntn Ntn Ntn
  | Block Name (Maybe Ntn) [Ntn] Span
  | Decl Name Ntn Span
  | Ident QName Span
  | Keyword Name Span
  | Field QName Span
  | Int Int Span
  | Tuple [Ntn] Span
  | Error Span

startPos :: Ntn -> Pos
startPos (App f _) = startPos f
startPos (Infix x _ _) = startPos x
startPos (Block _ _ _ s) = s.start
startPos (Decl _ _ s) = s.start
startPos (Ident _ s) = s.start
startPos (Keyword _ s) = s.start
startPos (Field _ s) = s.start
startPos (Int _ s) = s.start
startPos (Tuple _ s) = s.start
startPos (Error s) = s.start

endPos :: Ntn -> Pos
endPos (App _ x) = endPos x
endPos (Infix _ _ y) = endPos y
endPos (Block _ _ _ s) = s.end
endPos (Decl _ _ s) = s.end
endPos (Ident _ s) = s.end
endPos (Keyword _ s) = s.end
endPos (Field _ s) = s.end
endPos (Int _ s) = s.end
endPos (Tuple _ s) = s.end
endPos (Error s) = s.end

span :: Ntn -> Span
span n = Span (startPos n) (endPos n)

-- Debug printing for notation
--------------------------------------------------------------------------------

head :: Ntn -> Doc ann
head (App _ _) = "App"
head (Infix _ _ _) = "Infix"
head (Block x _ _ _) = "Block" <+> pretty x
head (Decl x _ _) = "Decl" <+> pretty x
head (Ident x _) = "Ident" <+> pretty x
head (Keyword x _) = "Keyword" <+> pretty x
head (Field x _) = "Field" <+> pretty x
head (Int i _) = "Int" <+> pretty i
head (Tuple _ _) = "Tuple"
head (Error _) = "Error"

children :: Ntn -> [Ntn]
children (App f x) = [f, x]
children (Infix x op y) = [x, op, y]
children (Block _ mh xs _) = maybeToList mh ++ xs
children (Decl _ x _) = [x]
children (Ident _ _) = []
children (Keyword _ _) = []
children (Field _ _) = []
children (Int _ _) = []
children (Tuple ns _) = ns
children (Error _) = []

instance Pretty Ntn where
  pretty n = if null cs then h else vsep [h, indent 2 $ vsep cs]
   where
    h = head n <+> "(" <> pretty (span n) <> ")"
    cs = map pretty (children n)
