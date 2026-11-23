-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT
{-# OPTIONS_GHC -Wno-orphans #-}

module Coln.FLIR.Value where

import Coln.Common
import Coln.Core.Params
-- import Coln.Core.Print
import Coln.SIR.Realm qualified as SIR
import Coln.SIR.Syntax qualified as SIR

-- import Control.Arrow ((***))
-- import Data.Aeson qualified as AE
-- import Data.Aeson.Encoding qualified as AE
-- import Data.Char (toLower)
-- import Data.Map.Ordered qualified as OMap
-- import Data.Maybe (fromMaybe)
-- import Data.String (fromString)
-- import FNotation qualified as N
-- import FNotation.Kinds qualified as K
import GHC.Generics

newtype LocalName = LocalName { name :: Text }
  deriving (Show, Eq, Generic)

type ValType = SIR.ScalarType

data Materialization
  = Recomputed
  | Materialized
  deriving (Show, Eq, Generic)

type TableVariant = SIR.TableVariant

newtype Context = Context { bindings :: [(LocalName, ValType)] }
  deriving (Show, Eq, Generic)

nameAt :: Context -> FId -> LocalName
nameAt c (FId i) = fst $ c.bindings !! i

newtype PhysicalPrimaryKey = PhysicalPrimaryKey { columns :: Int }
  deriving (Show, Eq, Generic)

data Table = Table
  { tableVariant :: TableVariant PhysicalPrimaryKey
  , columns :: Context
  }
  deriving (Show, Eq, Generic)

data El
  = Lit Literal
  | LocalVar FId
  deriving (Show, Eq, Generic)

data Atom = Atom
  { table :: GlobalSymbol
  , rowId :: Maybe El
  , values :: [El]
  }
  deriving (Show, Eq, Generic)

data Prop
  = PAtom Atom
  | PEq ValType El El
  deriving (Show, Eq, Generic)

data Law = Law
  { lawVariant :: SIR.LawVariant
  , context :: Context
  , antecedents :: [Prop]
  , consequents :: [Prop]
  }

data Rule = Rule
  { context :: Context
  , antecedents :: [Prop]
  , consequent :: Atom
  }

data Query = Query
  { context :: Context
  , props :: [Prop]
  }

type Union = SIR.Union

data Realm = Realm
  { tables :: OMap GlobalSymbol Table
  , rules :: OMap GlobalSymbol Rule
  , laws :: OMap GlobalSymbol Law
  }

-- JSON
--------------------------------------------------------------------------------

-- aeOptions :: AE.Options
-- aeOptions =
--   AE.defaultOptions
--     { AE.allNullaryToStringTag = False
--     , AE.constructorTagModifier = \x -> fmap toLower (take 1 x) ++ (drop 1 x)
--     }

-- pathMapEncoding :: (a -> AE.Encoding) -> OMap GlobalSymbol a -> AE.Encoding
-- pathMapEncoding f = AE.list (\(k, v) -> AE.pairs $ AE.pair "path" (AE.toEncoding k) <> AE.pair "value" (f v)) . OMap.assocs

-- instance AE.ToJSON GlobalSymbol where
--   toJSON = panic "aesons behaving badly"
--   toEncoding = AE.toEncoding . (.name)

-- instance AE.ToJSON Materialization where
--   toEncoding = AE.genericToEncoding aeOptions{AE.allNullaryToStringTag = True}

-- instance AE.ToJSON TableVariant where
--   toJSON = panic "aesons behaving badly"
--   toEncoding = \case
--     Ground -> SIR.taggedEncoding "ground" $ mempty
--     Bundle -> SIR.taggedEncoding "cons" $ mempty
--     View m -> SIR.taggedEncoding "view" $ AE.pair "materialization" $ AE.toEncoding m

-- instance AE.ToJSON Table where
--   toJSON = panic "aesons behaving badly"
--   toEncoding e =
--     AE.pairs $
--       mconcat
--         [ AE.pair "tableVariant" $ AE.toEncoding e.tableVariant
--         , AE.pair "columns" $ AE.list (\(k, v) -> AE.pairs $ AE.pair "path" (AE.toEncoding k) <> AE.pair "type" (AE.toEncoding v)) e.columns
--         , AE.pair "primaryKey" $ fromMaybe AE.null_ $ fmap (AE.list AE.toEncoding) e.primaryKey
--         ]

-- instance AE.ToJSON El where
--   toJSON = panic "aesons behaving badly"
--   toEncoding = \case
--     Lit l -> SIR.taggedEncoding "lit" $ AE.pair "lit" $ case l of
--       LitInt i -> SIR.taggedEncoding "int" $ AE.pair "value" $ AE.toEncoding i
--       LitString s -> SIR.taggedEncoding "string" $ AE.pair "value" $ AE.toEncoding s
--     LocalVar (FId i) -> SIR.taggedEncoding "var" $ AE.pair "index" $ AE.toEncoding i

-- instance AE.ToJSON Atom where
--   toJSON = panic "aesons behaving badly"
--   toEncoding a =
--     AE.pairs $
--       mconcat
--         [ AE.pair "entity" $ AE.toEncoding a.entity
--         , AE.pair "rowId" $ AE.toEncoding a.rowId
--         , AE.pair "values" $ AE.list (\(i, t) -> AE.pairs $ mconcat [AE.pair "column" (AE.toEncoding i), AE.pair "term" (AE.toEncoding t)]) a.values
--         ]

-- instance AE.ToJSON Prop where
--   toEncoding = \case
--     PAtom a -> SIR.taggedEncoding "atom" $ AE.pair "atom" $ AE.toEncoding a
--     PEq l r -> SIR.taggedEncoding "eq" $ AE.pair "left" (AE.toEncoding l) <> AE.pair "right" (AE.toEncoding r)

-- instance AE.ToJSON Rule where
--   toJSON = panic "aesons behaving badly"
--   toEncoding r =
--     AE.pairs $
--       mconcat
--         [ AE.pair "vars" $ AE.list (AE.list id . (\(x, y) -> [x, y]) . (AE.toEncoding *** AE.toEncoding)) $ r.vars
--         , AE.pair "antecedents" $ AE.toEncoding r.antecedents
--         , AE.pair "definand" $ AE.toEncoding r.definand
--         , AE.pair "arguments" $ AE.toEncoding r.args
--         ]

-- instance AE.ToJSON Law where
--   toJSON = panic "aesons behaving badly"
--   toEncoding r =
--     AE.pairs $
--       mconcat
--         [ AE.pair "lawVariant" $ AE.toEncoding r.lawVariant
--         , AE.pair "vars" $ AE.list (AE.list id . (\(x, y) -> [x, y]) . (AE.toEncoding *** AE.toEncoding)) $ r.vars
--         , AE.pair "antecedents" $ AE.toEncoding r.antecedents
--         , AE.pair "consequents" $ AE.toEncoding r.consequents
--         ]

-- instance AE.ToJSON Realm where
--   toJSON = panic "aesons behaving badly"
--   toEncoding r = AE.pairs $ AE.pair "entities" (pathMapEncoding AE.toEncoding r.entities) <> AE.pair "rules" (pathMapEncoding AE.toEncoding r.rules) <> AE.pair "laws" (pathMapEncoding AE.toEncoding r.laws)

-- -- Pretty-printer
-- --------------------------------------------------------------------------------

-- tableVariantDeclKeyword :: TableVariant -> Name
-- tableVariantDeclKeyword e = Name [] $ case e of
--   Ground -> "ground"
--   Bundle -> "cons"
--   View _ -> "view" -- TODO

-- ruleVariantDeclKeyword :: SIR.LawVariant -> Name
-- ruleVariantDeclKeyword e = Name [] $ case e of
--   SIR.Enforced -> "enforced"
--   SIR.Monitored -> "monitored"

-- instance ToNotationTop GlobalSymbol where
--   toNotationTop tn = N.Raw tn.name ()

-- instance ToNotationTop ColType where
--   toNotationTop = \case
--     SIR.RowId e -> toNotationTop e
--     SIR.BuiltinTy bt -> N.Keyword (fromString $ show bt) ()

-- instance ToNotationTop (ColName, ColType) where
--   toNotationTop (n, t) = N.Infix (toNotationTop n) (N.Keyword ":" ()) (toNotationTop t)

-- instance ToNotationTop (GlobalSymbol, Table) where
--   toNotationTop (tn, e) = do
--     let keyword = tableVariantDeclKeyword e.tableVariant
--     let cols = N.Tuple (map toNotationTop e.columns) ()
--     let colsWKey = case e.primaryKey of
--           Nothing -> cols
--           Just primaryKey -> N.Infix cols (N.Keyword "primarykey" ()) (N.Tuple (map toNotationTop $ map (fst . (e.columns !!)) primaryKey) ())
--     N.Decl keyword (N.Infix (toNotationTop tn) (N.Keyword ":=" ()) colsWKey) ()

-- instance ToNotationTop Literal where
--   toNotationTop = \case
--     LitInt i -> N.Int i ()
--     LitString t -> N.String t ()

-- toNotationTerm :: [ColName] -> El -> N.Ntn0
-- toNotationTerm _ (Lit l) = toNotationTop l
-- toNotationTerm cs (LocalVar (FId i)) = toNotationTop (cs !! i)

-- toNotationAtom :: OMap GlobalSymbol [ColName] -> [ColName] -> Atom -> N.Ntn0
-- toNotationAtom columnNames cs a = do
--   let entity = toNotationTop a.entity
--   let cols = case OMap.lookup a.entity columnNames of
--         Just cols -> cols
--         Nothing -> panic $ show a.entity ++ " not found"
--   let field (i, t) = N.Infix (toNotationTop (cols !! i)) (N.Keyword "↦" ()) (toNotationTerm cs t)
--   let body = N.Juxt entity $ N.Tuple (map field a.values) ()
--   case a.rowId of
--     Nothing -> body
--     Just r -> N.Infix (toNotationTerm cs r) (N.Keyword "∈" ()) body

-- toNotationProp :: OMap GlobalSymbol [ColName] -> [ColName] -> Prop -> N.Ntn0
-- toNotationProp ts cs = \case
--   PAtom a -> toNotationAtom ts cs a
--   PEq a b -> N.Infix (toNotationTerm cs a) (N.Keyword "=" ()) (toNotationTerm cs b)

-- toNotationConjunction :: [N.Ntn0] -> N.Ntn0
-- toNotationConjunction [] = N.Keyword "⊤" ()
-- toNotationConjunction [p] = p
-- toNotationConjunction (p : ps) = N.Infix p (N.Keyword "∧" ()) (toNotationConjunction ps)
-- toNotationRule :: OMap GlobalSymbol [ColName] -> (GlobalSymbol, Rule) -> N.Ntn0
-- toNotationRule columnNames (tn, r) = do
--   let keyword = "chased"
--   let head = foldl' N.Juxt (toNotationTop tn) (fmap toNotationTop (map fst r.vars))
--   let ante = toNotationConjunction $ fmap (toNotationProp columnNames $ map fst r.vars) r.antecedents
--   let cons = toNotationAtom columnNames (map fst r.vars) $ Atom r.definand Nothing $ zip [0 ..] r.args
--   let seq = N.Infix ante (N.Keyword "⊢" ()) cons
--   N.Decl keyword (N.Infix head (N.Keyword ":=" ()) seq) ()

-- toNotationLaw :: OMap GlobalSymbol [ColName] -> (GlobalSymbol, Law) -> N.Ntn0
-- toNotationLaw columnNames (tn, r) = do
--   let keyword = ruleVariantDeclKeyword r.lawVariant
--   let head = foldl' N.Juxt (toNotationTop tn) (fmap toNotationTop (map fst r.vars))
--   let ante = toNotationConjunction $ fmap (toNotationProp columnNames $ map fst r.vars) r.antecedents
--   let cons = toNotationConjunction $ fmap (toNotationProp columnNames $ map fst r.vars) r.consequents
--   let seq = N.Infix ante (N.Keyword "⊢" ()) cons
--   N.Decl keyword (N.Infix head (N.Keyword ":=" ()) seq) ()

-- instance ToNotationTop Realm where
--   toNotationTop (Realm es _us ds rs) = do
--     let nes = N.Block "entities" Nothing (fmap toNotationTop (OMap.assocs es)) ()
--     let columnNames = fmap (fmap fst . (.columns)) es
--     let nds = N.Block "rules" Nothing (fmap (toNotationRule columnNames) (OMap.assocs ds)) ()
--     let nrs = N.Block "laws" Nothing (fmap (toNotationLaw columnNames) (OMap.assocs rs)) ()
--     N.Block "flatrealm" Nothing [nes, nds, nrs] ()

-- irLexConfig :: N.ConfTable K.Kind
-- irLexConfig =
--   N.confTableFromList
--     [ ("flatrealm", K.Block)
--     , ("entities", K.Block)
--     , ("definitions", K.Block)
--     , ("rules", K.Block)
--     , ("table", K.Decl)
--     , ("view", K.Decl)
--     , ("index", K.Decl)
--     , ("chased", K.Decl)
--     , ("enforced", K.Decl)
--     , ("monitored", K.Decl)
--     , ("end", K.End)
--     , (":=", K.SKeyword)
--     , ("=", K.SKeyword)
--     , (":", K.SKeyword)
--     , ("∈", K.SKeyword)
--     , ("∧", K.SKeyword)
--     , ("⊢", K.SKeyword)
--     , ("↦", K.SKeyword)
--     , ("⊤", K.SKeyword)
--     ]

-- irParseConfig :: N.ConfTable N.Prec
-- irParseConfig =
--   N.confTableFromList
--     [ (":=", N.Prec 10 N.AssocNon)
--     , (":", N.Prec 20 N.AssocNon)
--     , ("⊢", N.Prec 30 N.AssocNon)
--     , ("∧", N.Prec 35 N.AssocR)
--     , ("=", N.Prec 40 N.AssocNon)
--     , ("∈", N.Prec 45 N.AssocNon)
--     , ("↦", N.Prec 60 N.AssocNon)
--     ]

-- instance DPretty Realm where
--   dpretty r = N.dprettyWithConfigs irParseConfig irLexConfig $ toNotationTop r
