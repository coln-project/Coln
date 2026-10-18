module Coln.Util.Bwd where

import Coln.Util.Containers
import Coln.Util.Debug

infixl 5 :>

data Bwd a = BwdNil | Bwd a :> a
  deriving (Functor, Show, Eq, Ord)

newtype BId = BId Int
  deriving (Eq, Num, Show)

newtype FId = FId Int
  deriving (Show, Eq)

reverseId :: Int -> FId -> BId
reverseId n (FId i) = BId (n - i - 1)

instance ElemAt (Bwd a) BId a where
  elemAt BwdNil _ =
    panic "`elemAt xs i` should only be called if i is a valid index in xs"
  elemAt (_ :> x) (BId 0) = x
  elemAt (xs :> _) (BId i) = elemAt xs (BId (i - 1))

instance ToList (Bwd a) a where
  toList bwd = go bwd []
   where
    go BwdNil list = list
    go (bwd' :> x) list = go bwd' (x : list)

instance Foldable Bwd where
  foldr f z bwd = go z bwd
   where
    go acc BwdNil = acc
    go acc (bwd' :> x) = go (f x acc) bwd'

instance FromList (Bwd a) a where
  fromList xs = go xs BwdNil
   where
    go [] bwd = bwd
    go (x : xs') bwd = go xs' (bwd :> x)

instance Semigroup (Bwd a) where
  xs <> BwdNil = xs
  xs <> (ys :> y) = (xs <> ys) :> y

instance Monoid (Bwd a) where
  mempty = BwdNil
