-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

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
  | RowIdSet Access
  | ColnRef Access
  | TableCellRef Access
  | StoreHandle
  | TransactionHandle
  deriving (Show)
