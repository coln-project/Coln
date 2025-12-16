module Geolog.Common where

import Data.ByteString qualified as BS
import Data.Hashable
import Data.String (IsString)

newtype RawName = RawName BS.ByteString
  deriving (Eq, Ord, IsString, Hashable) via BS.ByteString

data Name = NRawName RawName | N_
  deriving (Eq, Ord)

type Bwd a = [a]

infixl 5 :>

pattern (:>) :: Bwd a -> a -> Bwd a
pattern xs :> x = x : xs

type Fwd a = [a]

infixr 5 :<

pattern (:<) :: a -> Fwd a -> Fwd a
pattern x :< xs = x : xs
