module FNotation.Trees where

import Data.Maybe (maybeToList)
import Data.Text (Text)
import Diagnostician
import FNotation.Names
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
data NtnGeneric a
  = App (NtnGeneric a) [NtnGeneric a]
  | Infix (NtnGeneric a) (NtnGeneric a) (NtnGeneric a)
  | Block Name (Maybe (NtnGeneric a)) [NtnGeneric a] a
  | -- a declaration with modifiers
    MDecl [Name] Name (NtnGeneric a) a
  | Tuple [NtnGeneric a] a
  | Ident Name a
  | Keyword Name a
  | Field Name a
  | Tag Name a
  | Int Int a
  | String Text a
  | Error a

pattern Decl :: Name -> NtnGeneric a -> a -> NtnGeneric a
pattern Decl x n s = MDecl [] x n s

type Ntn = NtnGeneric Span

type Ntn0 = NtnGeneric ()

startPos :: Ntn -> Pos
startPos (App f _) = startPos f
startPos (Infix x _ _) = startPos x
startPos (Block _ _ _ s) = s.start
startPos (MDecl _ _ _ s) = s.start
startPos (Ident _ s) = s.start
startPos (Keyword _ s) = s.start
startPos (Field _ s) = s.start
startPos (Tag _ s) = s.start
startPos (Int _ s) = s.start
startPos (String _ s) = s.start
startPos (Tuple _ s) = s.start
startPos (Error s) = s.start

endPos :: Ntn -> Pos
endPos (App _ xs) = endPos (last xs)
endPos (Infix _ _ y) = endPos y
endPos (Block _ _ _ s) = s.end
endPos (MDecl _ _ _ s) = s.end
endPos (Ident _ s) = s.end
endPos (Keyword _ s) = s.end
endPos (Field _ s) = s.end
endPos (Tag _ s) = s.end
endPos (Int _ s) = s.end
endPos (String _ s) = s.end
endPos (Tuple _ s) = s.end
endPos (Error s) = s.end

span :: Ntn -> Span
span n = Span (startPos n) (endPos n)

-- Debug printing for notation
--------------------------------------------------------------------------------

head :: Ntn -> DDoc
head (App _ _) = "App"
head (Infix _ _ _) = "Infix"
head (Block x _ _ _) = "Block" <+> dpretty x
head (MDecl mods x _ _) = "MDecl" <+> hsep (dpretty <$> mods ++ [x])
head (Ident x _) = "Ident" <+> dpretty x
head (Keyword x _) = "Keyword" <+> dpretty x
head (Field x _) = "Field" <+> dpretty x
head (Tag x _) = "Tag" <+> dpretty x
head (Int i _) = "Int" <+> pretty i
head (String i _) = "Int" <+> pretty i
head (Tuple _ _) = "Tuple"
head (Error _) = "Error"

children :: Ntn -> [Ntn]
children (App f xs) = f : xs
children (Infix x op y) = [x, op, y]
children (Block _ mh xs _) = maybeToList mh ++ xs
children (MDecl _ _ n _) = [n]
children (Ident _ _) = []
children (Keyword _ _) = []
children (Field _ _) = []
children (Tag _ _) = []
children (Int _ _) = []
children (String _ _) = []
children (Tuple ns _) = ns
children (Error _) = []

instance DPretty Ntn where
  dpretty n = if null cs then h else vsep [h, indent 2 $ vsep cs]
   where
    h = head n <+> "(" <> dpretty (span n) <> ")"
    cs = map dpretty (children n)
