-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT
{-# LANGUAGE DeriveGeneric #-}

module Coln.Backend.IR where

import Data.Aeson qualified as AE
import Data.Aeson.Encoding qualified as AE
import Data.Char (toLower)
import Data.Map qualified as Map
import Data.Map.Strict qualified as Map
import Data.Maybe (fromMaybe)
import Data.Set qualified as Set
import GHC.Generics

-- XXX Lit/BultinTy should probably be moved up in the hierarchy
import Coln.Common
import Coln.Core.Params

type ColName = Path

data ColType
  = RowId TableName
  | BuiltinTy BuiltinTy
  deriving (Show, Eq, Generic)

data Materialization
  = Recomputed
  | Memoized
  | Materialized
  deriving (Show, Eq, Generic)

data IndexMethod
  = BTree
  deriving (Show, Eq, Generic)

data EntityVariant
  = Table
  | View Materialization
  | Index IndexMethod [ColName]
  deriving (Show, Eq, Generic)

data Entity = Entity
  { entityVariant :: EntityVariant
  , -- , columns :: Trie ColType
    columns :: [(ColName, ColType)]
  , primaryKey :: Maybe (Set.Set ColName)
  }
  deriving (Show, Eq, Generic)

data Term
  = Lit Literal
  | Var FId
  deriving (Show, Eq, Generic)

data Atom = Atom
  { entity :: TableName
  , rowId :: Maybe Term
  , values :: Map Int Term
  }
  deriving (Show, Eq, Generic)

data Prop
  = PAtom Atom
  | PEq Term Term
  deriving (Show, Eq, Generic)

data RuleVariant = Chased | Enforced | Monitored
  deriving (Show, Eq, Generic)

data Rule = Rule
  { ruleVariant :: RuleVariant
  , varNames :: Bwd ColName
  , varTypes :: Bwd ColType
  , antecedents :: [Prop]
  , consequents :: [Prop]
  }
  deriving (Show, Eq, Generic)

data FlatRealm = FlatRealm
  { entities :: Map TableName Entity
  , rules :: Map TableName Rule
  }
  deriving (Show, Eq, Generic)

aeOptions :: AE.Options
aeOptions =
  AE.defaultOptions
    { AE.allNullaryToStringTag = False
    , AE.constructorTagModifier = \x -> fmap toLower (take 1 x) ++ (drop 1 x)
    }

class PathLike a where
  namesOf :: a -> [Name]

encName :: Name -> AE.Encoding
encName n = AE.list AE.toEncoding $ n.init ++ [n.last]

encPath :: (PathLike a) => a -> AE.Encoding
encPath = AE.list encName . namesOf

instance PathLike Path where namesOf = toList
instance PathLike TableName where namesOf tn = tn.realm : namesOf tn.path

pathMapEncoding :: (PathLike k) => (a -> AE.Encoding) -> Map k a -> AE.Encoding
pathMapEncoding f = AE.list (\(k, v) -> AE.pairs $ AE.pair "path" (encPath k) <> AE.pair "value" (f v)) . Map.toAscList

taggedEncoding :: Text -> AE.Series -> AE.Encoding
taggedEncoding t v = AE.pairs $ AE.pair "tag" (AE.toEncoding t) <> v

instance AE.ToJSON ColType where
  toJSON = panic "aesons behaving badly"
  toEncoding = \case
    RowId e -> taggedEncoding "rowId" $ AE.pair "path" $ encPath e
    BuiltinTy bt -> taggedEncoding "builtin" $ AE.pair "type" $ AE.genericToEncoding aeOptions{AE.allNullaryToStringTag = True} bt

instance AE.ToJSON Materialization where
  toEncoding = AE.genericToEncoding aeOptions{AE.allNullaryToStringTag = True}

instance AE.ToJSON IndexMethod where
  toEncoding = AE.genericToEncoding aeOptions{AE.allNullaryToStringTag = True}

instance AE.ToJSON EntityVariant where
  toJSON = panic "aesons behaving badly"
  toEncoding = \case
    Table -> taggedEncoding "table" $ mempty
    View m -> taggedEncoding "view" $ AE.pair "materialization" $ AE.toEncoding m
    Index m cs -> taggedEncoding "index" $ AE.pair "method" (AE.toEncoding m) <> AE.pair "columns" (AE.list encPath cs)

instance AE.ToJSON Entity where
  toJSON = panic "aesons behaving badly"
  toEncoding e =
    AE.pairs $
      mconcat
        [ AE.pair "entityVariant" $ AE.toEncoding e.entityVariant
        , AE.pair "columns" $ AE.list (\(k, v) -> AE.pairs $ AE.pair "path" (encPath k) <> AE.pair "type" (AE.toEncoding v)) e.columns
        , AE.pair "primaryKey" $ fromMaybe AE.null_ $ fmap (AE.list encPath) $ fmap Set.toAscList e.primaryKey
        ]

instance AE.ToJSON Term where
  toJSON = panic "aesons behaving badly"
  toEncoding = \case
    Lit (LitInt i) -> taggedEncoding "int" $ AE.pair "int" $ AE.toEncoding $ show i
    Lit (LitString s) -> taggedEncoding "string" $ AE.pair "string" $ AE.toEncoding s
    Var (FId i) -> taggedEncoding "var" $ AE.pair "index" $ AE.toEncoding i

instance AE.ToJSON Atom where
  toJSON = panic "aesons behaving badly"
  toEncoding a =
    AE.pairs $
      mconcat
        [ AE.pair "entity" $ encPath a.entity
        , AE.pair "rowId" $ AE.toEncoding a.rowId
        , AE.pair "values" $ AE.list (\(k, v) -> AE.pairs $ AE.pair "column" (AE.toEncoding k) <> AE.pair "term" (AE.toEncoding v)) $ Map.toAscList a.values
        ]

instance AE.ToJSON Prop where
  toEncoding = \case
    PAtom a -> taggedEncoding "atom" $ AE.pair "atom" $ AE.toEncoding a
    PEq l r -> taggedEncoding "eq" $ AE.pair "left" (AE.toEncoding l) <> AE.pair "right" (AE.toEncoding r)

instance AE.ToJSON RuleVariant where
  toEncoding = AE.genericToEncoding aeOptions{AE.allNullaryToStringTag = True}

instance AE.ToJSON Rule where
  toJSON = panic "aesons behaving badly"
  toEncoding r =
    AE.pairs $
      mconcat
        [ AE.pair "ruleVariant" $ AE.toEncoding r.ruleVariant
        , AE.pair "varNames" $ AE.list encPath $ toList r.varNames
        , AE.pair "varTypes" $ AE.list AE.toEncoding $ toList r.varTypes
        , AE.pair "antecedents" $ AE.toEncoding r.antecedents
        , AE.pair "consequents" $ AE.toEncoding r.consequents
        ]

instance AE.ToJSON FlatRealm where
  toJSON = panic "aesons behaving badly"
  toEncoding fr = AE.pairs $ AE.pair "entities" (pathMapEncoding AE.toEncoding fr.entities) <> AE.pair "rules" (pathMapEncoding AE.toEncoding fr.rules)

emptyFlatRealm :: FlatRealm
emptyFlatRealm = FlatRealm Map.empty Map.empty
