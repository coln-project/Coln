module Coln.Common
  ( module Diagnostician
  , module FNotation
  , module Data.Map
  , module Data.Kind
  , module Data.Vector.Strict
  , module Data.Text
  , module Prettyprinter
  , panic
  , unimplemented
  , unwrap
  , ElemAt (..)
  , Lookup (..)
  , Contains (..)
  , FromList (..)
  , ToList (..)
  , PartialOrd (..)
  , Reverse (..)
  , Bwd (..)
  , BId (..)
  , Fwd (..)
  , FId (..)
  , Dict (..)
  , KeyIndex (..)
  , dictLength
  , getKeyIndex
  , withHead
  , Trie (..)
  ) where

import Prelude hiding (lookup)
import Control.Monad.IO.Class
import Data.Hashable
import Data.Kind (Type, Constraint)
import Data.Map (Map)
import Data.Map qualified as Map
import Data.String (IsString, fromString)
import Data.Text (Text)
import Data.Text.Unsafe qualified as TU
import Data.Vector.Strict (Vector)
import Data.Vector.Generic qualified as VG
import Data.Vector.Generic.Mutable qualified as VGM
import Data.Vector.Hashtables (FrozenDictionary)
import Data.Vector.Hashtables qualified as HT
import Data.Vector.Strict qualified as V
import Diagnostician
import FNotation (Name(..))
import Prettyprinter (Pretty (..))
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

class ElemAt a i b | a i -> b where
  elemAt :: a -> i -> b

class Lookup a i b | a -> i b where
  lookup :: a -> i -> Maybe b

class Contains a i | a -> i where
  contains :: a -> i -> Bool

class ToList a e | a -> e where
  toList :: a -> [e]

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

-- TODO: write custom foldable instance which implements tail-recursive toList
data Bwd a = BwdNil | Bwd a :> a
  deriving (Functor, Eq)

newtype BId = BId Int
  deriving (Eq, Num, Show)

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

data DictHead = DictHead
  { byName :: Map Name Int
  , keys :: Vector Name
  }

data Dict a = Dict
  { head :: DictHead
  , values :: Vector a
  }

dictLength :: Dict a -> Int
dictLength d = V.length d.values

instance Lookup (Dict a) Name a where
  lookup d x = (d.values V.!) <$> Map.lookup x d.head.byName

instance ElemAt (Dict a) Name a where
  elemAt t x = unwrap $ lookup t x

instance FromList (Dict a) (Name, a) where
  fromList pairs = do
    let keys = V.fromList $ fst <$> pairs
    let values = V.fromList $ snd <$> pairs
    let byName = Map.fromList $ zip (fst <$> pairs) [0..]
    Dict (DictHead byName keys) values

instance Functor Dict where
  fmap f d = Dict d.head (fmap f d.values)

instance ToList (Dict a) (Name, a) where
  toList d = zip (V.toList d.head.keys) (V.toList d.values)

newtype KeyIndex = KeyIndex { value :: Int }

instance ElemAt (Dict a) KeyIndex a where
  elemAt d (KeyIndex i) = d.values V.! i

instance Contains (Dict a) Name where
  contains d x = Map.member x d.head.byName

getKeyIndex :: Dict a -> Name -> KeyIndex
getKeyIndex d x = KeyIndex $ d.head.byName Map.! x

withHead :: Dict a -> [b] -> Dict b
withHead d xs = Dict d.head (V.fromList xs)

-- Name-based Tries
--------------------------------------------------------------------------------

-- Generator trie
data Trie a
  = Leaf a
  | Node (Dict (Trie a))
  deriving (Functor)
