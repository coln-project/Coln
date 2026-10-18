module Coln.Util.Match where

import Data.Kind (Type)

data Match (f :: k -> Type) (g :: k -> Type) where
  Pair :: f i -> g i -> Match f g
