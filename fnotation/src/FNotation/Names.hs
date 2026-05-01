module FNotation.Names where

import Data.String (IsString, fromString)
import Data.Text (Text)
import Diagnostician
import Prettyprinter

-- | A name of the form `a/b/c`
--
-- The properties of a name (such as the precedence when used as an operator) are
-- determined by the *last* segment. Thus, `a/+` is an infix operator with the
-- same precedence as `+`.
--
-- This also means, for instance, that if `theory` is a keyword, then `fresh/theory`
-- is also a keyword.
data Name = Name
  { init :: [Text],
    last :: Text
  }
  deriving (Eq, Ord)

instance Show Name where
  show x = mconcat ((<> "/") . show <$> x.init) <> show x.last

instance DPretty Name where
  dpretty x = mconcat ((<> "/") . pretty <$> x.init) <> pretty x.last

instance IsString Name where
  fromString s = Name [] (fromString s)
