module Coln.Core.Realm where

import Coln.Common

type RealmId = Name
type Path = Bwd Name

data TableName = TableName { realm :: RealmId, path :: Path }
  deriving (Eq)
