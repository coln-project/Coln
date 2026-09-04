module Coln.FLIR.Value where

import Coln.Common
import Coln.Core.Params
import Coln.Core.Print
import Coln.SIR.Realm qualified as SIR
import Coln.SIR.Syntax qualified as SIR

import Control.Arrow ((***))
import Data.Aeson qualified as AE
import Data.Aeson.Encoding qualified as AE
import Data.Char (toLower)
import Data.Map.Ordered qualified as OMap
import Data.Maybe (fromJust, fromMaybe, mapMaybe)
import Data.String (fromString)
import FNotation qualified as N
import FNotation.Kinds qualified as K
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

-- Pretty-printer
--------------------------------------------------------------------------------

entityVariantDeclKeyword :: EntityVariant -> Name
entityVariantDeclKeyword e = Name [] $ case e of
  Table -> "table"
  View _ -> "view" -- TODO
  Index _ _ -> "index" -- TODO

ruleVariantDeclKeyword :: SIR.RuleVariant -> Name
ruleVariantDeclKeyword e = Name [] $ case e of
  SIR.Enforced -> "enforced"
  SIR.Monitored -> "monitored"

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
    SIR.RowId e -> toNotationTop e
    SIR.BuiltinTy bt -> N.Keyword (fromString $ show bt) ()

instance ToNotationTop (ColName, ColType) where
  toNotationTop (n, t) = N.Infix (toNotationColName n) (N.Keyword ":" ()) (toNotationTop t)

instance ToNotationTop (TableName, Entity) where
  toNotationTop (tn, e) = do
    let keyword = entityVariantDeclKeyword e.entityVariant
    let cols = N.Tuple (map toNotationTop e.columns) ()
    let colsWKey = case e.primaryKey of
          Nothing -> cols
          Just primaryKey -> N.Infix cols (N.Keyword "primarykey" ()) (N.Tuple (map toNotationColName $ map (fst . (e.columns !!)) primaryKey) ())
    N.Decl keyword (N.Infix (toNotationTop tn) (N.Keyword ":=" ()) colsWKey) ()

instance ToNotationTop Literal where
  toNotationTop = \case
    LitInt i -> N.Int i ()
    LitString t -> N.String t ()

toNotationTerm :: [ColName] -> El -> N.Ntn0
toNotationTerm _ (Lit l) = toNotationTop l
toNotationTerm cs (LocalVar (FId i)) = toNotationTop (cs !! i)
toNotationTerm _ (Param (FId _)) = panic "param"

toNotationAtom :: OMap TableName [ColName] -> [ColName] -> Atom -> N.Ntn0
toNotationAtom columnNames cs a = do
  let entity = toNotationTop a.entity
  let cols = fromJust (OMap.lookup a.entity columnNames)
  let field (i, t) = N.Infix (toNotationColName (cols !! i)) (N.Keyword "↦" ()) (toNotationTerm cs t)
  let body = N.Juxt entity $ N.Tuple (map field . mapMaybe sequence $ zip [0 ..] a.values) ()
  case a.rowId of
    Nothing -> body
    Just r -> N.Infix (toNotationTerm cs r) (N.Keyword "∈" ()) body

toNotationProp :: OMap TableName [ColName] -> [ColName] -> Prop -> N.Ntn0
toNotationProp ts cs = \case
  PAtom a -> toNotationAtom ts cs a
  PEq a b -> N.Infix (toNotationTerm cs a) (N.Keyword "=" ()) (toNotationTerm cs b)

toNotationConjunction :: [N.Ntn0] -> N.Ntn0
toNotationConjunction [] = N.Keyword "⊤" ()
toNotationConjunction [p] = p
toNotationConjunction (p : ps) = N.Infix p (N.Keyword "∧" ()) (toNotationConjunction ps)
toNotationDefinition :: OMap TableName [ColName] -> (TableName, Definition) -> N.Ntn0
toNotationDefinition columnNames (tn, r) = do
  let keyword = "chased"
  let head = foldl' N.Juxt (toNotationTop tn) (fmap toNotationTop (map fst r.vars))
  let ante = toNotationConjunction $ fmap (toNotationProp columnNames $ map fst r.vars) r.antecedents
  let cons = toNotationAtom columnNames (map fst r.vars) $ Atom r.definand Nothing $ map Just r.args
  let seq = N.Infix ante (N.Keyword "⊢" ()) cons
  N.Decl keyword (N.Infix head (N.Keyword ":=" ()) seq) ()

toNotationRule :: OMap TableName [ColName] -> (TableName, Rule) -> N.Ntn0
toNotationRule columnNames (tn, r) = do
  let keyword = ruleVariantDeclKeyword r.ruleVariant
  let head = foldl' N.Juxt (toNotationTop tn) (fmap toNotationTop (map fst r.vars))
  let ante = toNotationConjunction $ fmap (toNotationProp columnNames $ map fst r.vars) r.antecedents
  let cons = toNotationConjunction $ fmap (toNotationProp columnNames $ map fst r.vars) r.consequents
  let seq = N.Infix ante (N.Keyword "⊢" ()) cons
  N.Decl keyword (N.Infix head (N.Keyword ":=" ()) seq) ()

instance ToNotationTop Realm where
  toNotationTop (Realm es ds rs) = do
    let nes = N.Block "entities" Nothing (fmap toNotationTop (OMap.assocs es)) ()
    let columnNames = fmap (fmap fst . (.columns)) es
    let nds = N.Block "definitions" Nothing (fmap (toNotationDefinition columnNames) (OMap.assocs ds)) ()
    let nrs = N.Block "rules" Nothing (fmap (toNotationRule columnNames) (OMap.assocs rs)) ()
    N.Block "flatrealm" Nothing [nes, nds, nrs] ()

irLexConfig :: N.ConfTable K.Kind
irLexConfig =
  N.confTableFromList
    [ ("flatrealm", K.Block)
    , ("entities", K.Block)
    , ("definitions", K.Block)
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
  N.confTableFromList
    [ (":=", N.Prec 10 N.AssocNon)
    , (":", N.Prec 20 N.AssocNon)
    , ("⊢", N.Prec 30 N.AssocNon)
    , ("∧", N.Prec 35 N.AssocR)
    , ("=", N.Prec 40 N.AssocNon)
    , ("∈", N.Prec 45 N.AssocNon)
    , ("↦", N.Prec 60 N.AssocNon)
    ]

instance DPretty Realm where
  dpretty r = N.dprettyWithConfigs irParseConfig irLexConfig $ toNotationTop r
