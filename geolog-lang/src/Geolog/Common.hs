module Geolog.Common where

import Control.Monad.IO.Class
import Data.Hashable
import Data.Kind (Constraint)
import Data.String (IsString, fromString)
import Data.Text (Text)
import Data.Text.Unsafe qualified as TU
import Data.Vector.Generic qualified as VG
import Data.Vector.Generic.Mutable qualified as VGM
import Data.Vector.Hashtables (FrozenDictionary)
import Data.Vector.Hashtables qualified as HT
import Data.Vector.Strict qualified as V
import Prettyprinter
import System.IO.Unsafe (unsafePerformIO)

#ifdef DEBUG
import GHC.Stack
#endif

-- Panics
--------------------------------------------------------------------------------

#ifdef DEBUG
type Dbg = HasCallStack
#else
type Dbg = () :: Constraint
#endif

panic :: (Dbg) => String -> a
panic invariant = error $ "invariant violated: " ++ invariant

unimplemented :: (Dbg) => a
unimplemented = error "unimplemented"

unwrap :: Maybe a -> a
unwrap (Just x) = x
unwrap Nothing = panic "should only unwrap a Just"

-- Container classes
--------------------------------------------------------------------------------

class ElemAt a i b | a -> i b where
  elemAt :: a -> i -> b

class Lookup a i b | a -> i b where
  lookup :: a -> i -> Maybe b

class Contains a i | a -> i where
  contains :: a -> i -> Bool

class ToList t where
  toList :: t a -> [a]

class FromList a e | a -> e where
  fromList :: [e] -> a

-- Partial orderings
--------------------------------------------------------------------------------

class PartialOrd a where
  leq :: a -> a -> Bool

-- Forward and backwards lists
--------------------------------------------------------------------------------

class Reverse a b | a -> b where
  rev :: a -> b

infixl 5 :>

data Bwd a = BwdNil | Bwd a :> a
  deriving (Functor)

newtype BId = BId Int
  deriving (Eq, Num, Show)

instance ElemAt (Bwd a) BId a where
  elemAt BwdNil _ =
    panic "`elemAt xs i` should only be called if i is a valid index in xs"
  elemAt (_ :> x) (BId 0) = x
  elemAt (xs :> _) (BId i) = elemAt xs (BId (i - 1))

instance ToList Bwd where
  toList xs = go xs []
   where
    go BwdNil l = l
    go (xs' :> x) l = go xs' (x : l)

instance Semigroup (Bwd a) where
  xs <> BwdNil = xs
  xs <> (ys :> y) = (xs <> ys) :> y

instance Monoid (Bwd a) where
  mempty = BwdNil

data MeasuredBwd a = MeasuredBwd
  { values :: Bwd a
  , length :: Int
  }

(++>) :: MeasuredBwd a -> a -> MeasuredBwd a
(++>) (MeasuredBwd xs n) x = MeasuredBwd (xs :> x) (n + 1)

infixr 5 :<

data Fwd a = FwdNil | a :< Fwd a

newtype FId = FId Int
  deriving (Eq)

instance ElemAt (Fwd a) FId a where
  elemAt FwdNil _ =
    panic "`elemAt xs i` should only be called if i is a valid index in xs"
  elemAt (x :< _) (FId 0) = x
  elemAt (_ :< xs) (FId i) = elemAt xs (FId (i - 1))

instance Reverse (Bwd a) (Fwd a) where
  rev = go FwdNil
   where
    go xs' BwdNil = xs'
    go xs' (xs :> x) = go (x :< xs') xs

instance Reverse (Fwd a) (Bwd a) where
  rev = go BwdNil
   where
    go xs' FwdNil = xs'
    go xs' (x :< xs) = go (xs' :> x) xs
