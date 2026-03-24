module FNotation.Config where

import Data.Text
import Data.Vector.Hashtables (FrozenDictionary)
import Data.Vector.Hashtables qualified as HT
import Data.Vector.Strict qualified as V
import System.IO.Unsafe (unsafePerformIO)

newtype ConfTable a = ConfTable (FrozenDictionary V.Vector Text V.Vector a)

confTableLookup :: ConfTable a -> Text -> Maybe a
confTableLookup (ConfTable d) x = case HT.findElem d x of
  -1 -> Nothing
  i -> Just (HT.fvalue d V.! i)

confTableFromList :: [(Text, a)] -> ConfTable a
confTableFromList l = unsafePerformIO do
  d <- HT.fromList l
  fd <- HT.unsafeFreeze d
  pure $ ConfTable fd

data Assoc = AssocL | AssocR | AssocNon
  deriving (Eq, Show)

data Prec = Prec
  { binding :: Int
  , assoc :: Assoc
  }
  deriving (Eq, Show)

precLe :: Prec -> Prec -> Maybe Bool
precLe (Prec b a) (Prec b' a')
  | b < b' = Just True
  | b > b' = Just False
  | otherwise = case (a, a') of
      (AssocL, AssocL) -> Just False
      (AssocR, AssocR) -> Just True
      _ -> Nothing
