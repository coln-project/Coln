module Coln.Util.PartialOrd where

class PartialOrd a where
  leq :: a -> a -> Bool
