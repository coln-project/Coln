module Coln.Backend.TypeScript.Params where

data Access
  = View
  | Transaction
  deriving (Show)

accessLevels :: [Access]
accessLevels = [View, Transaction]

extends :: Access -> Maybe Access
extends = \case
  View -> Nothing
  Transaction -> Just View

data RuntimeConst
  = Value
  | ColnSet Access
  | StoreHandle
  | TransactionHandle
  deriving (Show)
