-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT
{-# LANGUAGE DeriveGeneric #-}
{-# OPTIONS_GHC -fno-warn-orphans #-}

module Coln.Backend.IR where

-- XXX Lit/BultinTy should probably be moved up in the hierarchy
import Coln.Common
import Coln.Core.Params
import Coln.Core.Print
import Data.Aeson qualified as AE
import Data.Aeson.Encoding qualified as AE
import Data.Char (toLower)
import Data.List (foldl', intercalate)
import Data.Map.Ordered (OMap)
import Data.Map.Ordered qualified as OMap
import Data.Maybe (fromJust, fromMaybe)
import Data.Set qualified as Set
import Data.String (fromString)
import FNotation as N
import FNotation.Kinds as K
import GHC.Generics

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
  , values :: OMap Int Term
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
  { entities :: OMap TableName Entity
  , rules :: OMap TableName Rule
  }
  deriving (Show, Eq, Generic)

emptyFlatRealm :: FlatRealm
emptyFlatRealm = FlatRealm OMap.empty OMap.empty

-- JSON
--------------------------------------------------------------------------------

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

pathMapEncoding :: (PathLike k) => (a -> AE.Encoding) -> OMap k a -> AE.Encoding
pathMapEncoding f = AE.list (\(k, v) -> AE.pairs $ AE.pair "path" (encPath k) <> AE.pair "value" (f v)) . OMap.assocs

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
        , AE.pair "values" $ AE.list (\(k, v) -> AE.pairs $ AE.pair "column" (AE.toEncoding k) <> AE.pair "term" (AE.toEncoding v)) $ OMap.assocs a.values
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

-- Pretty-printer
--------------------------------------------------------------------------------

entityVariantDeclKeyword :: EntityVariant -> Name
entityVariantDeclKeyword e = Name [] $ fromString $ case e of
  Table -> "table"
  View _ -> "view" -- TODO
  Index _ _ -> "index" -- TODO

ruleVariantDeclKeyword :: RuleVariant -> Name
ruleVariantDeclKeyword e = Name [] $ fromString $ case e of
  Chased -> "chased"
  Enforced -> "enforced"
  Monitored -> "monitored"

toNotationColName :: ColName -> N.Ntn0
toNotationColName BwdNil = N.Tuple [] () -- Shouldn't happen
toNotationColName (BwdNil :> x) = N.Field x ()
toNotationColName (p :> x) = N.Juxt (toNotationColName p) (N.Field x ())

instance ToNotationTop Path where
  toNotationTop BwdNil = N.Tuple [] () -- Shouldn't happen
  toNotationTop (BwdNil :> x) = N.Ident x ()
  toNotationTop (p :> x) = N.Juxt (toNotationTop p) (N.Field x ())

instance ToNotationTop TableName where
  toNotationTop tn = foldl (\n p -> N.Juxt n (N.Field p ())) (N.Ident "ℜ" ()) tn.path

instance ToNotationTop ColType where
  toNotationTop = \case
    RowId e -> toNotationTop e
    BuiltinTy bt -> N.Keyword (fromString $ show bt) ()

instance ToNotationTop (ColName, ColType) where
  toNotationTop (n, t) = N.Infix (toNotationColName n) (N.Keyword ":" ()) (toNotationTop t)

instance ToNotationTop (TableName, Entity) where
  toNotationTop (tn, e) = do
    let keyword = entityVariantDeclKeyword e.entityVariant
    let cols = N.Tuple (map toNotationTop e.columns) ()
    let colsWKey = case e.primaryKey of
          Nothing -> cols
          Just primaryKey -> N.Infix cols (N.Keyword "primarykey" ()) (N.Tuple (map toNotationColName $ Set.toList primaryKey) ())
    N.Decl keyword (N.Infix (toNotationTop tn) (N.Keyword ":=" ()) colsWKey) ()

instance ToNotationTop Literal where
  toNotationTop = \case
    LitInt i -> N.Int i ()
    LitString t -> N.String t ()

toNotationTerm :: Bwd ColName -> Term -> N.Ntn0
toNotationTerm _ (Lit l) = toNotationTop l
toNotationTerm cs (Var i) = toNotationTop (elemAt (rev cs) i) -- TODO: Why does Rule use Bwd and FId together?

toNotationAtom :: OMap TableName [ColName] -> Bwd ColName -> Atom -> N.Ntn0
toNotationAtom columnNames cs a = do
  let entity = toNotationTop a.entity
  let cols = fromJust (OMap.lookup a.entity columnNames)
  let field (i, t) = N.Infix (toNotationColName (cols !! i)) (N.Keyword "↦" ()) (toNotationTerm cs t)
  let body = N.Juxt entity $ N.Tuple (field <$> OMap.assocs a.values) ()
  case a.rowId of
    Nothing -> body
    Just r -> N.Infix (toNotationTerm cs r) (N.Keyword "∈" ()) body

toNotationProp :: OMap TableName [ColName] -> Bwd ColName -> Prop -> N.Ntn0
toNotationProp ts cs = \case
  PAtom a -> toNotationAtom ts cs a
  PEq a b -> N.Infix (toNotationTerm cs a) (N.Keyword "=" ()) (toNotationTerm cs b)

toNotationConjunction :: [N.Ntn0] -> N.Ntn0
toNotationConjunction [] = N.Keyword "⊤" ()
toNotationConjunction [p] = p
toNotationConjunction (p : ps) = N.Infix p (N.Keyword "∧" ()) (toNotationConjunction ps)

toNotationRule :: OMap TableName [ColName] -> (TableName, Rule) -> N.Ntn0
toNotationRule columnNames (tn, r) = do
  let keyword = ruleVariantDeclKeyword r.ruleVariant
  let head = foldl' N.Juxt (toNotationTop tn) (fmap toNotationTop (toList r.varNames))
  let ante = toNotationConjunction $ fmap (toNotationProp columnNames r.varNames) r.antecedents
  let cons = toNotationConjunction $ fmap (toNotationProp columnNames r.varNames) r.consequents
  let seq = N.Infix ante (N.Keyword "⊢" ()) cons
  N.Decl keyword (N.Infix head (N.Keyword ":=" ()) seq) ()

instance ToNotationTop FlatRealm where
  toNotationTop (FlatRealm es rs) = do
    let nes = N.Block "entities" Nothing (fmap toNotationTop (OMap.assocs es)) ()
    let columnNames = fmap (fmap fst . (.columns)) es
    let nrs = N.Block "rules" Nothing (fmap (toNotationRule columnNames) (OMap.assocs rs)) ()
    N.Block "flatrealm" Nothing [nes, nrs] ()

irLexConfig :: N.ConfTable Kind
irLexConfig =
  confTableFromList
    [ ("flatrealm", K.Block)
    , ("entities", K.Block)
    , ("rules", K.Block)
    , ("table", K.Decl)
    , ("view", K.Decl)
    , ("index", K.Decl)
    , ("chased", K.Decl)
    , ("enforced", K.Decl)
    , ("monitored", K.Decl)
    , ("end", K.End)
    , (":=", K.SKeyword)
    , ("=", K.SKeyword)
    , (":", K.SKeyword)
    , ("∈", K.SKeyword)
    , ("∧", K.SKeyword)
    , ("⊢", K.SKeyword)
    , ("↦", K.SKeyword)
    , ("⊤", K.SKeyword)
    ]

irParseConfig :: N.ConfTable N.Prec
irParseConfig =
  confTableFromList
    [ (":=", Prec 10 AssocNon)
    , (":", Prec 20 AssocNon)
    , ("⊢", Prec 30 AssocNon)
    , ("∧", Prec 35 AssocR)
    , ("=", Prec 40 AssocNon)
    , ("∈", Prec 45 AssocNon)
    , ("↦", Prec 60 AssocNon)
    ]

instance DPretty FlatRealm where
  dpretty r = N.dprettyWithConfigs irParseConfig irLexConfig $ toNotationTop r
