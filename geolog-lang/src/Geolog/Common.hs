module Geolog.Common where

import Data.Vector.Hashtables (FrozenDictionary)
import Data.Vector.Hashtables qualified as HT
import Data.Vector.Strict as V
import Symbolize (Symbol)
import System.IO.Unsafe (unsafePerformIO)

type Name = Symbol

type Bwd a = [a]

infixl 5 :>

pattern (:>) :: Bwd a -> a -> Bwd a
pattern xs :> x = x : xs

type Fwd a = [a]

infixr 5 :<

pattern (:<) :: a -> Fwd a -> Fwd a
pattern x :< xs = x : xs

newtype ConfTable v = ConfTable (FrozenDictionary V.Vector Name V.Vector v)

class ElemAt a i b | a -> i b where
  elemAt :: a -> i -> b

class Lookup a i b | a -> i b where
  lookup :: a -> i -> Maybe b

class Contains a i | a -> i where
  contains :: a -> i -> Bool

instance Lookup (ConfTable v) Name v where
  lookup (ConfTable d) x = case HT.findElem d x of
    -1 -> Nothing
    i -> Just (HT.fvalue d V.! i)

class FromList a e | a -> e where
  fromList :: [e] -> a

instance FromList (ConfTable v) (Name, v) where
  fromList l = unsafePerformIO do
    d <- HT.fromList l
    fd <- HT.unsafeFreeze d
    pure $ ConfTable fd
