module Coln.FLIR.Serialization where

import Coln.Common
import Coln.Core.Params
import Coln.FLIR.Value
import Coln.FLIR.Sexp qualified as S
import Coln.SIR.Syntax qualified as SIR
import Coln.SIR.Realm qualified as SIR

import Data.Map.Ordered qualified as OMap

class ToSexp a where
  toSexp :: a -> S.Sexp ()

class ToSexps a where
  toSexps :: a -> [S.Sexp ()]

class ToSexpInContext a where
  toSexp' :: Context -> a -> S.Sexp ()

atom_ :: S.Atom -> S.Sexp ()
atom_ a = S.Atom a ()

group_ :: [S.Sexp ()] -> S.Sexp ()
group_ exprs = S.Group exprs ()

instance ToSexp Int where
  toSexp i = atom_ $ S.Int i

instance ToSexp GlobalSymbol where
  toSexp s = atom_ $ S.Symbol s.name

instance ToSexp LocalName where
  toSexp s = atom_ $ S.VarId s.name

instance ToSexp ValType where
  toSexp = \case
    SIR.RowId s -> toSexp s
    SIR.BuiltinTy s -> toSexp s

instance ToSexp BuiltinTy where
  toSexp = \case
    BuiltinInt -> "int"
    BuiltinString -> "string"

instance ToSexp TableVariant where
  toSexp v = case v of
    Ground -> "ground"
    Cons -> "cons"
    View _ -> "view"

instance ToSexp a => ToSexp [a] where
  toSexp = group_ . fmap toSexp
  
instance ToSexpInContext a => ToSexpInContext [a] where
  toSexp' c = group_ . fmap (toSexp' c)

instance (ToSexp a, ToSexp b) => ToSexp (a, b) where
  toSexp (x, y) = group_ [toSexp x, toSexp y]

instance ToSexp Context where
  toSexp c = toSexp c.bindings

instance ToSexp PrimaryKey where
  toSexp pk = group_ ["primarykey", toSexp pk.columns]

optional :: Maybe a -> [a]
optional (Just x) = [x]
optional Nothing = []

instance ToSexp Table where
  toSexp t = group_ $
    [ toSexp t.tableVariant
    , toSexp t.columns
    ] ++ optional (toSexp <$> t.primaryKey)

instance ToSexp Union where
  toSexp u = group_ $
    [ "union"
    , toSexp $ toList u.variants
    ]

instance ToSexp Literal where
  toSexp = \case
    LitString s -> atom_ $ S.String s
    LitInt i -> atom_ $ S.Int i

instance ToSexpInContext El where
  toSexp' c = \case
    Lit l -> toSexp l
    LocalVar i -> toSexp $ nameAt c i
    
instance ToSexpInContext Atom where
  toSexp' c a = group_ $
    [toSexp a.entity] ++
    optional (fmap (\t -> group_ ["id", toSexp' c t]) a.rowId) ++
    (toSexp' c <$> a.values)

instance ToSexpInContext Prop where
  toSexp' c = \case
    PAtom a -> toSexp' c a
    PEq a t0 t1 -> group_ ["=", toSexp a, toSexp' c t0, toSexp' c t1]

instance ToSexp SIR.LawVariant where
  toSexp = \case
    SIR.Enforced -> "enforced"
    SIR.Monitored -> "monitored"

instance ToSexp Law where
  toSexp l = group_
    [ "law"
    , toSexp l.lawVariant
    , toSexp l.context
    , toSexp' l.context l.antecedents
    , toSexp' l.context l.consequents
    ]

instance ToSexp Rule where
  toSexp r = group_
    [ "rule"
    , toSexp r.context
    , toSexp' r.context r.antecedents
    , group_ $ [toSexp r.definand] ++ fmap (toSexp' r.context) r.args
    ]

instance ToSexp Query where
  toSexp q = group_
    [ "query"
    , toSexp q.context
    , toSexp' q.context q.props
    ]

instance ToSexp a => ToSexps (OMap GlobalSymbol a) where
  toSexps m = toSexp <$> OMap.assocs m

instance ToSexps Realm where
  toSexps r =
    toSexps r.tables ++
    toSexps r.unions ++
    toSexps r.rules ++
    toSexps r.laws
