{-# LANGUAGE UndecidableInstances #-}

{- |
This module contains the definition for geolog core.

Core is what we elaborate to. It is broken up into four datatypes

+---------+--------+-------+
|         | Syntax | Value |
+=========+========+=======+
| Element | `ElS`  | `ElV` |
+---------+--------+-------+
| Type    | `TyS`  | `TyV` |
+---------+--------+-------+

Each of these datatypes is indexed by a `Level` parameter. There are four
levels:

- `Query` (Kovacs's @Sort@)
- `Theory` (Kovacs's @Sig@)
- `Meta` (Kovacs's @Set@)
- `Prim` (Kovacs's @C@)

So in total, there are 16 different combinations of datatype and level;
it's a good thing we don't need separate datatypes for each of these! In
the original Rust EMTT implementation, we did in fact have eight datatypes
(we only had the query and theory levels).

This is similar to [Type-Theoretic Signatures for Algebraic Theories and
Inductive Types](https://andraskovacs.github.io/pdfs/wg6stockholm.pdf), except
we only have /universes/ for sorts and theories. That is, we have 8 judgment
formers (elements and types on each level). Then the type/element judgments for
queries are internalized in a theory-level universe (`QueryU`) and the
type/element judgments for theories are internalized in a meta-level universe
(`TheoryU`).

An alternative to this would be something like [Fuss-free universe
hierarchies](https://www.jonmsterling.com/01HX/). However, in geolog each level
is its own special snowflake, with slightly different type constructors, so I
think that it makes sense to really separate them out with completely different
judgments.
-}
module Geolog.Core where

import Data.Kind (Type)
import Data.Singletons.TH
import Geolog.Common
import Prettyprinter

-- Levels
--------------------------------------------------------------------------------

$(singletons [d|data Level = Query | Theory | Meta | Prim|])

instance Show Level where
  show = \case
    Query -> "Query"
    Theory -> "Theory"
    Meta -> "Meta"
    Prim -> "Prim"

instance Pretty Level where
  pretty = pretty . show

-- TODO: Does this data structure already exist in the singletons library?
data Any :: (Level -> Type) -> Type where
  Any :: Sing l -> f l -> Any f

levelOf :: Any f -> Level
levelOf (Any s _) = fromSing s

extractAt :: Sing l -> Any f -> f l
extractAt s (Any s' a) = case (s, s') of
  (SQuery, SQuery) -> a
  (STheory, STheory) -> a
  (SMeta, SMeta) -> a
  (SPrim, SPrim) -> a
  _ -> error "tried to extract at a non-matching level"

extract :: forall l f. (SingI l) => Any f -> f l
extract a = extractAt (sing :: Sing l) a

data LevelInclusion :: Level -> Level -> Type where
  QueryInTheory :: LevelInclusion Query Theory
  QueryInMeta :: LevelInclusion Query Meta
  TheoryInMeta :: LevelInclusion Theory Meta
  PrimInMeta :: LevelInclusion Prim Meta

deriving instance Show (LevelInclusion l l')

liDom :: LevelInclusion l l' -> Sing l
liDom QueryInTheory = SQuery
liDom QueryInMeta = SQuery
liDom TheoryInMeta = STheory
liDom PrimInMeta = SPrim

withDom :: LevelInclusion l l' -> ((SingI l) => a) -> a
withDom QueryInTheory x = x
withDom QueryInMeta x = x
withDom TheoryInMeta x = x
withDom PrimInMeta x = x

-- Core syntax
--------------------------------------------------------------------------------

data Abs f l = Abs QName (f l)
  deriving (Show)

data Fields f l = Fields [(QName, f l)]
  deriving (Show)

instance ElemAt (Fields f l) QName (f l) where
  elemAt (Fields []) _ = impossible
  elemAt (Fields ((x, v) : fs)) x'
    | x == x' = v
    | otherwise = elemAt (Fields fs) x'

data ElS :: Level -> Type where
  Var :: BId -> ElS l
  QueryCode :: TyS Query -> ElS Theory
  TheoryCode :: TyS Theory -> ElS Meta
  TheoryApp :: ElS Theory -> ElS Query -> ElS Theory
  TheoryLam :: Abs ElS Theory -> ElS Theory
  MetaApp :: ElS Meta -> ElS Meta -> ElS Meta
  MetaLam :: Abs ElS Meta -> ElS Meta
  Proj :: ElS l -> QName -> ElS l
  Cons :: Fields ElS l -> ElS l
  LiftEl :: ElS l -> LevelInclusion l l' -> ElS l'

deriving instance Show (ElS l)

data TyS :: Level -> Type where
  QueryU :: TyS Theory
  QueryEl :: ElS Theory -> TyS Query
  TheoryU :: TyS Meta
  TheoryEl :: ElS Meta -> TyS Theory
  TheoryPi :: TyS Query -> Abs TyS Theory -> TyS Theory
  MetaPi :: TyS Meta -> Abs TyS Meta -> TyS Meta
  Record :: Fields TyS l -> TyS l
  LiftTy :: TyS l -> LevelInclusion l l' -> TyS l'

-- For debugging
deriving instance Show (TyS l)

-- Core values
--------------------------------------------------------------------------------

type Env = Bwd (Any ElV)

data Clo f l = Clo Env QName (f l)

data Spine :: Level -> Type where
  SId :: Spine l
  STheoryApp :: Spine Theory -> ElV Query -> Spine Theory
  SMetaApp :: Spine Meta -> ElV Meta -> Spine Meta
  SProj :: Spine l -> QName -> Spine l

data ElV :: Level -> Type where
  VNeu :: FId -> Spine l -> ElV l
  VQueryCode :: TyV Query -> ElV Theory
  VTheoryCode :: TyV Theory -> ElV Meta
  VLiftEl :: ElV l -> LevelInclusion l l' -> ElV l'
  VTheoryLam :: Clo ElS Theory -> ElV Theory
  VMetaLam :: Clo ElS Meta -> ElV Meta
  VCons :: Fields ElV l -> ElV l

data TyV :: Level -> Type where
  VQueryU :: TyV Theory
  VQueryEl :: ElV Theory -> TyV Query
  VTheoryU :: TyV Meta
  VTheoryEl :: ElV Meta -> TyV Theory
  VTheoryPi :: TyV Query -> Clo TyS Theory -> TyV Theory
  VMetaPi :: TyV Meta -> Clo TyS Meta -> TyV Meta
  VRecord :: Env -> Fields TyS l -> TyV l
  VLiftTy :: TyV l -> LevelInclusion l l' -> TyV l'
