module Coln.FLIR.Value where

import Coln.Common
import Coln.Core.Params
import Coln.SIR.Realm qualified as SIR
import Coln.SIR.Syntax qualified as SIR

import Control.Arrow ((***))
import Data.Aeson qualified as AE
import Data.Aeson.Encoding qualified as AE
import Data.Char (toLower)
import Data.Map.Ordered qualified as OMap
import Data.Maybe (fromMaybe)
import Data.Set qualified as Set
import GHC.Generics

type ColName = Path

type ColType = SIR.ScalarType

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
  , columns :: [(ColName, ColType)]
  , primaryKey :: Maybe [Int]
  }
  deriving (Show, Eq, Generic)

-- Param only shows up in queries, not in the FLIR for a realm
data El
  = Lit Literal
  | LocalVar FId
  | Param FId
  deriving (Show, Eq, Generic)

data Atom = Atom
  { entity :: TableName
  , rowId :: Maybe El
  , values :: [Maybe El]
  }
  deriving (Show, Eq, Generic)

data Prop
  = PAtom Atom
  | PEq El El
  deriving (Show, Eq, Generic)

data Rule = Rule
  { ruleVariant :: SIR.RuleVariant
  , vars :: [(ColName, ColType)]
  , antecedents :: [Prop]
  , consequents :: [Prop]
  }

data Definition = Definition
  { vars :: [(ColName, ColType)]
  , antecedents :: [Prop]
  , definand :: TableName
  , args :: [El]
  }

data Query = Query
  { vars :: [(ColName, ColType)]
  , props :: [Prop]
  }

data Realm = Realm
  { entities :: OMap TableName Entity
  , definitions :: OMap TableName Definition
  , rules :: OMap TableName Rule
  }

-- JSON
--------------------------------------------------------------------------------

aeOptions :: AE.Options
aeOptions =
  AE.defaultOptions
    { AE.allNullaryToStringTag = False
    , AE.constructorTagModifier = \x -> fmap toLower (take 1 x) ++ (drop 1 x)
    }

pathMapEncoding :: (SIR.PathLike k) => (a -> AE.Encoding) -> OMap k a -> AE.Encoding
pathMapEncoding f = AE.list (\(k, v) -> AE.pairs $ AE.pair "path" (SIR.encPath k) <> AE.pair "value" (f v)) . OMap.assocs

instance AE.ToJSON Materialization where
  toEncoding = AE.genericToEncoding aeOptions{AE.allNullaryToStringTag = True}

instance AE.ToJSON IndexMethod where
  toEncoding = AE.genericToEncoding aeOptions{AE.allNullaryToStringTag = True}

instance AE.ToJSON EntityVariant where
  toJSON = panic "aesons behaving badly"
  toEncoding = \case
    Table -> SIR.taggedEncoding "table" $ mempty
    View m -> SIR.taggedEncoding "view" $ AE.pair "materialization" $ AE.toEncoding m
    Index m cs -> SIR.taggedEncoding "index" $ AE.pair "method" (AE.toEncoding m) <> AE.pair "columns" (AE.list SIR.encPath cs)

instance AE.ToJSON Entity where
  toJSON = panic "aesons behaving badly"
  toEncoding e =
    AE.pairs $
      mconcat
        [ AE.pair "entityVariant" $ AE.toEncoding e.entityVariant
        , AE.pair "columns" $ AE.list (\(k, v) -> AE.pairs $ AE.pair "path" (SIR.encPath k) <> AE.pair "type" (AE.toEncoding v)) e.columns
        , AE.pair "primaryKey" $ fromMaybe AE.null_ $ fmap (AE.list AE.toEncoding) e.primaryKey
        ]

instance AE.ToJSON El where
  toJSON = panic "aesons behaving badly"
  toEncoding = \case
    Lit l -> SIR.taggedEncoding "lit" $ AE.pair "lit" $ case l of
      LitInt i -> SIR.taggedEncoding "int" $ AE.pair "value" $ AE.toEncoding i
      LitString s -> SIR.taggedEncoding "string" $ AE.pair "value" $ AE.toEncoding s
    LocalVar (FId i) -> SIR.taggedEncoding "var" $ AE.pair "index" $ AE.toEncoding i
    Param (FId i) -> SIR.taggedEncoding "param" $ AE.pair "index" $ AE.toEncoding i

instance AE.ToJSON Atom where
  toJSON = panic "aesons behaving badly"
  toEncoding a =
    AE.pairs $
      mconcat
        [ AE.pair "entity" $ SIR.encPath a.entity
        , AE.pair "rowId" $ AE.toEncoding a.rowId
        , AE.pair "values" $ AE.toEncoding a.values
        ]

instance AE.ToJSON Prop where
  toEncoding = \case
    PAtom a -> SIR.taggedEncoding "atom" $ AE.pair "atom" $ AE.toEncoding a
    PEq l r -> SIR.taggedEncoding "eq" $ AE.pair "left" (AE.toEncoding l) <> AE.pair "right" (AE.toEncoding r)

instance AE.ToJSON Definition where
  toJSON = panic "aesons behaving badly"
  toEncoding r =
    AE.pairs $
      mconcat
        [ AE.pair "vars" $ AE.list (AE.list id . (\(x, y) -> [x, y]) . (SIR.encPath *** AE.toEncoding)) $ r.vars
        , AE.pair "antecedents" $ AE.toEncoding r.antecedents
        , AE.pair "definand" $ SIR.encPath r.definand
        , AE.pair "antecedents" $ AE.toEncoding r.args
        ]

instance AE.ToJSON Rule where
  toJSON = panic "aesons behaving badly"
  toEncoding r =
    AE.pairs $
      mconcat
        [ AE.pair "ruleVariant" $ AE.toEncoding r.ruleVariant
        , AE.pair "vars" $ AE.list (AE.list id . (\(x, y) -> [x, y]) . (SIR.encPath *** AE.toEncoding)) $ r.vars
        , AE.pair "antecedents" $ AE.toEncoding r.antecedents
        , AE.pair "consequents" $ AE.toEncoding r.consequents
        ]

instance AE.ToJSON Realm where
  toJSON = panic "aesons behaving badly"
  toEncoding r = AE.pairs $ AE.pair "entities" (pathMapEncoding AE.toEncoding r.entities) <> AE.pair "definitions" (pathMapEncoding AE.toEncoding r.definitions) <> AE.pair "rules" (pathMapEncoding AE.toEncoding r.rules)
