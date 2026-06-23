-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module FNotation.Names (
  Name (..),
  dprettyWithKinds,
  dprettyOpWithKinds,
) where

import Data.Char (isDigit, isLetter)
import Data.List (intersperse)
import Data.String (IsString, fromString)
import Data.Text (Text)
import Data.Text qualified as T
import Diagnostician
import FNotation.Config (ConfTable, confTableLookup)
import FNotation.Kinds (Kind (..))
import Prettyprinter

{- | A name of the form `a/b/c`

The properties of a name (such as the precedence when used as an operator) are
determined by the *last* segment. Thus, `a/+` is an infix operator with the
same precedence as `+`.  If you quote the `+` with backticks, however, it will parse as a simple identifier with the same name.

Using a keyword as the last segment of a name is an error unless the keyword is quoted with backticks, in which case it is still not a keyword but an ordinary identifier.
-}
data Name = Name
  { init :: [Text]
  , last :: Text
  }
  deriving (Eq, Ord)

instance Show Name where
  show x = mconcat ((<> "/") . T.unpack <$> x.init) <> T.unpack x.last

instance DPretty Name where
  dpretty x = mconcat ((<> "/") . pretty <$> x.init) <> pretty x.last

pand :: (a -> Bool) -> (a -> Bool) -> (a -> Bool)
pand f g x = f x && g x

por :: (a -> Bool) -> (a -> Bool) -> (a -> Bool)
por f g x = f x || g x

dprettyWithKinds :: ConfTable Kind -> Name -> DDoc
dprettyWithKinds config x = mconcat . intersperse "/" . map (namesegWithKinds config) $ x.init ++ [x.last]

dprettyOpWithKinds :: ConfTable Kind -> Name -> DDoc
dprettyOpWithKinds config x = mconcat . intersperse "/" $ (namesegWithKinds config `map` x.init) ++ [pretty x.last]

namesegWithKinds :: ConfTable Kind -> Text -> DDoc
namesegWithKinds config x = case confTableLookup config x of
  (Nothing; Just AIdent)
    | T.all (isLetter `por` isDigit `por` ('_' ==) `por` ('-' ==)) `pand` (T.any (isLetter `por` ('_' ==)) . T.take 1) $ x ->
        pretty x
  _ -> "`" <> pretty x <> "`"

instance IsString Name where
  fromString s = Name [] (fromString s)
