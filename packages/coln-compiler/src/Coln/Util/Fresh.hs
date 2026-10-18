-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Util.Fresh where

import Data.Map.Strict qualified as Map
import Data.Set qualified as Set
import Data.String (IsString (..))
import FNotation (Name (..))

import Coln.Util.Bwd
import Coln.Util.Containers
import Coln.Util.Dict

-- XXX should really be a stream so we statically know there are always fresh
alphaStrings :: [String]
alphaStrings = [xs [x] | xs <- map (++) $ "" : alphaStrings, x <- ['a' .. 'z']]

alphaNames :: [Name]
alphaNames = map fromString alphaStrings

class HasNames a where
  namesIn :: a -> Set.Set Name

instance HasNames DictHead where
  namesIn h = Map.keysSet h.byName

instance HasNames (Dict a) where
  namesIn h = namesIn h.head

instance HasNames [Name] where
  namesIn xs = Set.fromList xs

instance HasNames (Bwd Name) where
  namesIn xs = Set.fromList (toList xs)

instance HasNames (Set.Set Name) where
  namesIn = id

freshNamesFor :: (HasNames a) => a -> [Name]
freshNamesFor a = flip filter alphaNames $ flip Set.notMember $ namesIn a

freshNameFor :: (HasNames a) => a -> Name
freshNameFor = head . freshNamesFor

freshNameWithPref :: (HasNames a) => a -> Maybe Name -> Name
freshNameWithPref a (Just x) = case Set.member x (namesIn a) of
  True -> freshNameFor a
  False -> x
freshNameWithPref a Nothing = freshNameFor a

freshenBy :: Name -> String -> Name
freshenBy (Name qual last) s = Name (qual ++ [last]) (fromString s)

freshenFor :: (HasNames a) => a -> Name -> Name
freshenFor a x =
  head $
    filter
      (\x -> not $ Set.member x (namesIn a))
      (x : (freshenBy x <$> alphaStrings))
