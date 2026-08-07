-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT
{-# LANGUAGE AllowAmbiguousTypes #-}
{-# LANGUAGE DefaultSignatures #-}
{-# OPTIONS_GHC -Wno-orphans #-}

{- | Tests for the Haskell bindings to the Rust coln-store library.

There are three kinds of tests:

* __Golden tests__ compile @.coln@ source files (from @coln-compiler@'s test
  suite) to 'FlatRealm' via the lowering pass, then exercise the FFI
  round-trip and produce @.json@, @.debug@ and @.store@ golden files.

* __Round-trip tests__ use hand-built 'FlatRealm' values to exercise IR
  constructors that the compiler doesn't yet lower (views, indexes, builtin
  types, literals, equality propositions, chased rules).  These are purely
  in-memory: Haskell serialises to JSON, Rust deserialises and re-serialises,
  Haskell parses back, and the result is compared with 'Eq' on JSON values.

* __Generated tests__ do the same round-trip, but over a corpus of 'FlatRealm's
  derived from the /shape/ of the IR via "GHC.Generics", together with a check
  that the corpus really does use every constructor reachable from 'FlatRealm'.
  See the \"Constructor coverage\" section below: this is what stops the two
  sets of hand-written tests above from silently going out of date as the IR
  grows.
-}
module Main (main) where

import Coln.Backend.IR
import Coln.Backend.Lower (lowerRealm)
import Coln.Common (Bwd (..), FId (..))
import Coln.Core (Globals (..))
import Coln.Core.Params (BuiltinTy (..), Literal (..), Path, TableName (..))
import Coln.Diagnostics (ColnCode)
import Coln.Frontend.Parser (topFromText)
import Coln.Store.Wrapped
import Control.Monad (unless)
import Data.Aeson qualified as Aeson
import Data.Aeson.Encode.Pretty qualified as Aeson
import Data.ByteString.Lazy qualified as LBS
import Data.Foldable (for_, toList)
import Data.IORef (newIORef, readIORef)
import Data.Kind (Type)
import Data.List (sort)
import Data.Map.Ordered (OMap)
import Data.Map.Ordered qualified as OMap
import Data.Maybe (fromMaybe)
import Data.Proxy (Proxy (..))
import Data.Set (Set)
import Data.Set qualified as Set
import Data.Text (Text)
import Data.Text qualified as T
import Data.Text.Encoding (decodeUtf8, encodeUtf8)
import Data.Text.IO.Utf8 qualified as T
import Data.Traversable (for)
import Diagnostician (Diagnostic, dpretty, newFile, pureReporter)
import FNotation.Names (Name (..))
import GHC.Generics (
  C,
  Constructor (conName),
  D,
  Datatype (datatypeName),
  Generic (Rep, from, to),
  K1 (..),
  M1 (..),
  Meta,
  S,
  U1 (..),
  V1,
  type (:*:) (..),
  type (:+:) (..),
 )
import Prettyprinter (defaultLayoutOptions, layoutPretty)
import Prettyprinter.Render.Text (renderStrict)
import System.Directory (listDirectory)
import System.FilePath
import Test.Tasty
import Test.Tasty.Golden
import Test.Tasty.HUnit

-- TODO idea was to do full round-tripping, but we don't have `FromJSON`s...
-- so for now every round-trip has to go out through Rust and back
-- (`Haskell encode -> Rust decode -> Rust encode -> compare JSON values`),
-- which can't distinguish "both sides agree" from "both sides are wrong":
-- that's what the golden files are for.
--
-- TODO is there a binary format we should be testing as well?
-- Yes: `coln-store`'s `commit/wire/root.rs` encodes the same schemas and rules
-- as the JSON codec, independently and with no shared code, and nothing checks
-- that the two agree.  We can't reach it from here yet, because
--
--   * `serialize_root`/`deserialize_root` are `pub(crate)`, and
--   * the FFI hands out no store bytes, so the only public route in is
--     `Store::try_from_theory`, which first validates the theory.
--
-- Closing this needs `store_to_bytes`/`store_from_bytes` (or a chunks API) on
-- the FFI, which store persistence will want anyway.  Note that going in via
-- `Store` restricts us to semantically valid theories, so it would cover the
-- compiled tests below but not the generated corpus.
-- https://github.com/coln-project/Coln/issues/94
main :: IO ()
main = do
  compiled <- compiledTests
  defaultMain $
    testGroup
      "coln-store-hs"
      [ testGroup "compiled" compiled
      , testGroup "round-trip" roundTripTests
      , testGroup "generated" generatedTests
      ]

--------------------------------------------------------------------------------
-- Compiled tests: .coln → lower → FlatRealm → FFI round-trip + golden files
--------------------------------------------------------------------------------

{- | .coln files known NOT to define any realm (and therefore not lowerable).
Everything else in the golden directory is compiled and tested.
-}
colnFilesWithoutRealms :: [String]
colnFilesWithoutRealms =
  [ "ancestor"
  , "elaborator-errors"
  , "equality"
  , "fb3"
  , "hlevel"
  , "ind"
  , "lambda"
  , "magma"
  , "mathieu"
  , "modular-lattice"
  , "monoid-glog"
  , "nominal"
  , "or"
  , "paths" -- has a realm declaration but it fails to parse
  , "ssa"
  , "weighted"
  ]

compilerGoldenDir :: FilePath
compilerGoldenDir = "../coln-compiler/test/golden"

compiledTests :: IO [TestTree]
compiledTests = do
  allColnFiles <- sort . filter ((== ".coln") . takeExtension) <$> listDirectory compilerGoldenDir
  let candidates =
        [ (takeBaseName f, compilerGoldenDir </> f)
        | f <- allColnFiles
        , takeBaseName f `notElem` colnFilesWithoutRealms
        ]
  pure (map mkCompiledTest candidates <> [loweredCoverageTest candidates])

{- | A golden report of which IR constructors the lowering pass actually emits
for the @.coln@ corpus, and which it never does.

This isn't a correctness check, it's documentation of where the real coverage
comes from: everything in the second list is only ever seen by the synthetic
theories below, so if the JSON encoding of one of them is wrong, only those
tests can catch it.  It also gives a visible diff when the lowerer learns to
emit something new.
-}
loweredCoverageTest :: [(String, FilePath)] -> TestTree
loweredCoverageTest candidates =
  goldenVsString "lowered-coverage" "test/outputs/lowered-coverage.txt" do
    realms <- concat <$> for candidates (loadFlatRealms . snd)
    let emitted = foldMap (coverUsed . snd) realms
        reachable = coverAll (Proxy @FlatRealm)
    pure . LBS.fromStrict . encodeUtf8 . T.unlines $
      ("# emitted by the lowering pass (" <> T.pack (show (length candidates)) <> " .coln files)")
        : map showConRef (Set.toList emitted)
          <> [ ""
             , "# never emitted, so only covered by the synthetic theories"
             ]
          <> map showConRef (Set.toList (reachable Set.\\ emitted))
 where
  showConRef (ty, con) = "  " <> ty <> "." <> con

mkCompiledTest :: (String, FilePath) -> TestTree
mkCompiledTest (baseName, colnFile) =
  withResource (loadFlatRealms colnFile) mempty \getRealms ->
    testGroup baseName $
      [ testCase "has realms" do
          realms <- getRealms
          assertBool
            ("expected at least one realm in " <> colnFile)
            (not (null realms))
      , withResource
          ( do
              realms <- getRealms
              -- Concatenate all realm outputs for a single golden comparison.
              pure (concatFlatRealmOutputs realms)
          )
          mempty
          (goldenTestsFor ("test/outputs" </> baseName))
      ]

-- | Concatenate JSON, debug, and store outputs for all realms in a file.
concatFlatRealmOutputs :: [(String, FlatRealm)] -> ConcatenatedOutputs
concatFlatRealmOutputs realms =
  ConcatenatedOutputs
    { coJson = LBS.intercalate "\n" [prettyJson (Aeson.encode fr) | (_, fr) <- realms]
    , coDebug = [] -- filled in lazily by the test
    , coStore = [] -- filled in lazily by the test
    , coFlatRealms = realms
    }

data ConcatenatedOutputs = ConcatenatedOutputs
  { coJson :: LBS.ByteString
  , coDebug :: [LBS.ByteString] -- unused, populated per-realm
  , coStore :: [LBS.ByteString] -- unused, populated per-realm
  , coFlatRealms :: [(String, FlatRealm)]
  }

goldenTestsFor :: FilePath -> IO ConcatenatedOutputs -> TestTree
goldenTestsFor outputDir getOutputs =
  testGroup
    (takeFileName outputDir)
    [ goldenVsString "json" (outputDir <.> "json") do
        outputs <- getOutputs
        pure outputs.coJson
    , withResource
        ( do
            outputs <- getOutputs
            -- Parse all realms through Rust
            for outputs.coFlatRealms \(realmName, fr) -> do
              let json = decodeUtf8 (LBS.toStrict (Aeson.encode fr))
              theory <-
                either
                  ( \err ->
                      assertFailure $
                        "Rust failed to parse realm "
                          <> realmName
                          <> ": "
                          <> T.unpack (decodeUtf8 err)
                  )
                  pure
                  =<< theoryFromJson json
              pure (realmName, fr, theory)
        )
        mempty
        \getTheories ->
          testGroup
            "rust"
            [ testCase "round-trip" do
                theories <- getTheories
                for_ theories \(realmName, fr, theory) -> do
                  let rustJson = LBS.fromStrict (theoryToJson theory)
                  let origJson = Aeson.encode fr
                  assertEqual
                    ("JSON mismatch for realm " <> realmName)
                    (jsonValue origJson)
                    (jsonValue rustJson)
            , goldenVsString "debug" (outputDir <.> "debug") do
                theories <- getTheories
                pure $
                  LBS.intercalate "\n" [LBS.fromStrict (theoryDebug t) | (_, _, t) <- theories]
            , goldenVsString "store" (outputDir <.> "store") do
                theories <- getTheories
                chunks <- for theories \(realmName, _, theory) ->
                  either
                    ( \err ->
                        assertFailure $
                          "Rust failed to build store for realm "
                            <> realmName
                            <> ": "
                            <> T.unpack (decodeUtf8 err)
                    )
                    (pure . LBS.fromStrict . storeDump)
                    =<< storeFromTheory theory
                pure $ LBS.intercalate "\n" chunks
            ]
    ]

--------------------------------------------------------------------------------
-- Round-trip tests: hand-built FlatRealm → JSON → Rust → JSON → compare
--------------------------------------------------------------------------------

roundTripTests :: [TestTree]
roundTripTests =
  [ roundTrip "builtins" singleTableTheory
  , roundTrip "literals" literalsTheory
  , roundTrip "all-variants" allVariantsTheory
  , roundTrip "multi-segment-names" multiSegNameTheory
  , roundTrip "empty" emptyFlatRealm
  ]

roundTrip :: String -> FlatRealm -> TestTree
roundTrip name fr = testCase name (assertRoundTrips name fr)

-- | Haskell → JSON → Rust parse → Rust serialise → JSON → compare as Value.
assertRoundTrips :: String -> FlatRealm -> Assertion
assertRoundTrips what fr = do
  let hsJson = Aeson.encode fr
  rustTheory <-
    either
      (\err -> assertFailure $ "Rust parse failed for " <> what <> ": " <> T.unpack (decodeUtf8 err))
      pure
      =<< theoryFromJson (decodeUtf8 (LBS.toStrict hsJson))
  let rustJson = LBS.fromStrict (theoryToJson rustTheory)
  assertEqual ("JSON round-trip mismatch for " <> what) (jsonValue hsJson) (jsonValue rustJson)

--------------------------------------------------------------------------------
-- Constructor coverage: generated FlatRealms
--
-- The hand-written theories above only cover what someone remembered to write
-- down, and nothing tells us when they stop being enough.  So instead of
-- (only) enumerating values, we enumerate the *type*: 'Cover' derives, from the
-- shape of the IR, both
--
--   * the set of every constructor reachable from 'FlatRealm', and
--   * a finite corpus of 'FlatRealm's which between them use all of them,
--
-- and then asserts that those two agree.  Add a constructor to the IR (or a
-- field to a record) and the corpus grows to cover it automatically, so the
-- round-trip starts exercising it without anyone editing this file.  Add
-- one that the generic machinery *can't* reach (because some type has a
-- hand-written 'Cover' instance that wasn't updated) and the coverage test
-- fails, telling you exactly which constructor is missing.
--
-- Container positions report pseudo-constructors, e.g. @("[Prop]", "[]")@, so
-- "every constructor is covered" also implies that every list, 'Set', 'OMap',
-- 'Bwd' and 'Maybe' in the IR is exercised both empty and non-empty.
--------------------------------------------------------------------------------

generatedTests :: [TestTree]
generatedTests =
  [ testCase "covers every constructor" do
      let expected = coverAll (Proxy @FlatRealm)
          used = foldMap coverUsed generatedCorpus
      assertEqual
        "constructors reachable from FlatRealm but not used by the generated corpus"
        []
        (Set.toList (expected Set.\\ used))
      assertEqual
        "constructors used by the generated corpus but not reachable from FlatRealm"
        []
        (Set.toList (used Set.\\ expected))
  , -- One theory per line, so that a diff points at the theory that changed.
    goldenVsString "jsonl" "test/outputs/generated.jsonl" $
      pure (foldMap ((<> "\n") . Aeson.encode) generatedCorpus)
  , -- The count is in the name so that the test log shows how much is actually
    -- being round-tripped, and so that it moving is visible in CI output.
    testCase ("round-trip " <> show (length generatedCorpus) <> " theories") $
      for_ (zip [1 :: Int ..] generatedCorpus) \(i, fr) ->
        assertRoundTrips ("corpus line " <> show i) fr
  ]

generatedCorpus :: [FlatRealm]
generatedCorpus = coverCorpus

-- | A constructor, identified by the name of its type and its own name.
type ConRef = (Text, Text)

{- | Types whose constructors we want to be sure the tests cover.

Every method is derivable via "GHC.Generics", so for an IR type that derives
'Generic' an empty instance suffices.  Containers and abstract types (t'Text',
'Set', 'OMap', ...) need hand-written instances, which is exactly why 'coverAll'
and 'coverUsed' are separate: the former says what /should/ be covered, the
latter what actually /is/, and the test compares the two.
-}
class Cover a where
  -- | Name of the type, used to qualify constructor names.
  coverName :: Proxy a -> Text
  default coverName :: (GName (Rep a)) => Proxy a -> Text
  coverName _ = gName (Proxy @(Rep a))

  -- | Every constructor reachable from this type, transitively.
  coverAll :: Proxy a -> Set ConRef
  default coverAll :: (Generic a, GCover (Rep a)) => Proxy a -> Set ConRef
  coverAll p = gAll (coverName p) (Proxy @(Rep a))

  -- | The constructors used by a particular value.
  coverUsed :: a -> Set ConRef
  default coverUsed :: (Generic a, GCover (Rep a)) => a -> Set ConRef
  coverUsed x = gUsed (coverName (Proxy @a)) (from x)

  {- | A finite, non-empty corpus of values which between them use every
  constructor in 'coverAll'.
  -}
  coverCorpus :: [a]
  default coverCorpus :: (Generic a, GCover (Rep a)) => [a]
  coverCorpus = map to gCorpus

instance Cover ColType
instance Cover Materialization
instance Cover IndexMethod
instance Cover EntityVariant
instance Cover Entity
instance Cover Term
instance Cover Atom
instance Cover Prop
instance Cover RuleVariant
instance Cover Rule
instance Cover FlatRealm
deriving instance Generic Literal
instance Cover Literal
instance Cover BuiltinTy

instance Cover Int where
  coverName _ = "Int"
  coverAll _ = mempty
  coverUsed _ = mempty

  -- the extremes matter: the Rust side stores these as `i64`
  coverCorpus = [0, 1, -1, maxBound, minBound]

instance Cover Text where
  coverName _ = "Text"
  coverAll _ = mempty
  coverUsed _ = mempty

  -- escaping, non-ASCII, and a non-BMP character (a surrogate pair in UTF-16)
  coverCorpus = ["x", "a b", "", "π∀𝕏", "quote\"backslash\\", "nl\nl"]

instance Cover FId where
  coverName _ = "FId"
  coverAll _ = coverAll (Proxy @Int)
  coverUsed (FId i) = coverUsed i
  coverCorpus = map FId coverCorpus

instance Cover Name where
  coverName _ = "Name"
  coverAll _ = coverAll (Proxy @[Text]) <> coverAll (Proxy @Text)
  coverUsed n = coverUsed n.init <> coverUsed n.last
  coverCorpus = zipCycle (\init' last' -> Name{init = init', last = last'}) coverCorpus coverCorpus

instance Cover TableName where
  coverName _ = "TableName"
  coverAll _ = coverAll (Proxy @Name) <> coverAll (Proxy @Path)
  coverUsed t = coverUsed t.realm <> coverUsed t.path
  coverCorpus = zipCycle (\realm path' -> TableName{realm, path = path'}) coverCorpus coverCorpus

instance (Cover a, Cover b) => Cover (a, b) where
  coverName _ = "(" <> coverName (Proxy @a) <> ", " <> coverName (Proxy @b) <> ")"
  coverAll _ = coverAll (Proxy @a) <> coverAll (Proxy @b)
  coverUsed (a, b) = coverUsed a <> coverUsed b
  coverCorpus = zipCycle (,) coverCorpus coverCorpus

instance (Cover a) => Cover [a] where
  coverName _ = "[" <> coverName (Proxy @a) <> "]"
  coverAll p = containerAll p ("[]", "(:)") (Proxy @a)
  coverUsed = containerUsed (Proxy @[a]) ("[]", "(:)") . toList
  coverCorpus = containerCorpus id

instance (Cover a) => Cover (Maybe a) where
  coverName _ = applied "Maybe" [coverName (Proxy @a)]
  coverAll p = containerAll p ("Nothing", "Just") (Proxy @a)
  coverUsed = containerUsed (Proxy @(Maybe a)) ("Nothing", "Just") . toList
  coverCorpus = Nothing : map Just coverCorpus

instance (Cover a) => Cover (Bwd a) where
  coverName _ = applied "Bwd" [coverName (Proxy @a)]
  coverAll p = containerAll p ("BwdNil", "(:>)") (Proxy @a)
  coverUsed = containerUsed (Proxy @(Bwd a)) ("BwdNil", "(:>)") . toList
  coverCorpus = containerCorpus bwd

instance (Ord a, Cover a) => Cover (Set a) where
  coverName _ = applied "Set" [coverName (Proxy @a)]
  coverAll p = containerAll p ("empty", "nonEmpty") (Proxy @a)
  coverUsed = containerUsed (Proxy @(Set a)) ("empty", "nonEmpty") . toList
  coverCorpus = containerCorpus Set.fromList

instance (Ord k, Cover k, Cover v) => Cover (OMap k v) where
  coverName _ = applied "OMap" [coverName (Proxy @k), coverName (Proxy @v)]
  coverAll p = containerAll p ("empty", "nonEmpty") (Proxy @(k, v))
  coverUsed = containerUsed (Proxy @(OMap k v)) ("empty", "nonEmpty") . OMap.assocs
  coverCorpus = containerCorpus omap

-- | Render a type application, parenthesising arguments as necessary.
applied :: Text -> [Text] -> Text
applied f xs = T.unwords (f : map paren xs)
 where
  paren x = if T.any (== ' ') x then "(" <> x <> ")" else x

-- | The pseudo-constructors of a container, plus everything in its elements.
containerAll :: (Cover a, Cover e) => Proxy a -> ConRef -> Proxy e -> Set ConRef
containerAll p (emptyCon, nonEmptyCon) e =
  Set.fromList [(coverName p, emptyCon), (coverName p, nonEmptyCon)] <> coverAll e

-- | Which of a container's pseudo-constructors a value uses, plus its elements'.
containerUsed :: (Cover a, Cover e) => Proxy a -> ConRef -> [e] -> Set ConRef
containerUsed p (emptyCon, nonEmptyCon) xs =
  Set.insert (coverName p, if null xs then emptyCon else nonEmptyCon) (foldMap coverUsed xs)

{- | Empty, every singleton, and (if there is more than one element to choose
from) one two-element container, so that separators and ordering get exercised.
-}
containerCorpus :: forall e a. (Cover e) => ([e] -> a) -> [a]
containerCorpus build =
  map build $ [] : map pure elems <> [pair | let pair = take 2 elems, length pair == 2]
 where
  elems = coverCorpus @e

{- | Combine two corpora without the combinatorial explosion of a full product:
the result is only as long as the longer input, but by cycling the shorter one
we still guarantee that every element of both appears at least once.
-}
zipCycle :: (a -> b -> c) -> [a] -> [b] -> [c]
zipCycle f xs ys
  | null xs || null ys = []
  | otherwise = take (max (length xs) (length ys)) (zipWith f (cycle xs) (cycle ys))

--------------------------------------------------------------------------------
-- Generic implementation of 'Cover'
--------------------------------------------------------------------------------

-- | The name of a datatype, from its generic representation.
class GName (f :: Type -> Type) where
  gName :: Proxy f -> Text

instance (Datatype d) => GName (M1 D d f) where
  gName _ = T.pack (datatypeName (undefined :: M1 D d f ()))

class GCover (f :: Type -> Type) where
  -- | All constructors, qualified by the given (enclosing) datatype name.
  gAll :: Text -> Proxy f -> Set ConRef

  gUsed :: Text -> f x -> Set ConRef
  gCorpus :: [f x]

instance (GCover f) => GCover (M1 D d f) where
  gAll t _ = gAll t (Proxy @f)
  gUsed t (M1 x) = gUsed t x
  gCorpus = map M1 gCorpus

instance (Constructor c, GCover f) => GCover (M1 C c f) where
  gAll t _ = Set.insert (t, conNameOf (Proxy @c)) (gAll t (Proxy @f))
  gUsed t (M1 x) = Set.insert (t, conNameOf (Proxy @c)) (gUsed t x)
  gCorpus = map M1 gCorpus

instance (GCover f) => GCover (M1 S s f) where
  gAll t _ = gAll t (Proxy @f)
  gUsed t (M1 x) = gUsed t x
  gCorpus = map M1 gCorpus

instance (Cover c) => GCover (K1 i c) where
  gAll _ _ = coverAll (Proxy @c)
  gUsed _ (K1 x) = coverUsed x
  gCorpus = map K1 coverCorpus

instance GCover U1 where
  gAll _ _ = mempty
  gUsed _ _ = mempty
  gCorpus = [U1]

instance GCover V1 where
  gAll _ _ = mempty
  gUsed _ _ = mempty
  gCorpus = []

instance (GCover f, GCover g) => GCover (f :+: g) where
  gAll t _ = gAll t (Proxy @f) <> gAll t (Proxy @g)
  gUsed t = \case
    L1 x -> gUsed t x
    R1 x -> gUsed t x
  gCorpus = map L1 gCorpus <> map R1 gCorpus

instance (GCover f, GCover g) => GCover (f :*: g) where
  gAll t _ = gAll t (Proxy @f) <> gAll t (Proxy @g)
  gUsed t (x :*: y) = gUsed t x <> gUsed t y
  gCorpus = zipCycle (:*:) gCorpus gCorpus

conNameOf :: forall (c :: Meta). (Constructor c) => Proxy c -> Text
conNameOf _ = T.pack (conName (undefined :: M1 C c U1 ()))

--------------------------------------------------------------------------------
-- Loading .coln files
--------------------------------------------------------------------------------

{- | Compile a .coln file and return all lowered FlatRealms with realm names.

Any diagnostic reported while compiling (parse errors, elaboration failures,
etc.) fails the test loudly instead of silently dropping the offending
declaration: 'Coln.Frontend.Parser.Top.tryDecl' recovers from errors by
discarding the failed top-level declaration and carrying on, which is the
right behaviour for a language server but means a broken golden fixture would
otherwise show up as a confusing golden-file mismatch (or fewer realms than
expected) with no indication of why.
-}
loadFlatRealms :: FilePath -> IO [(String, FlatRealm)]
loadFlatRealms colnFile = do
  src <- T.readFile colnFile
  let f = newFile colnFile src
  diagsRef <- newIORef []
  globals <- topFromText (pureReporter diagsRef) f
  diags <- reverse <$> readIORef diagsRef
  unless (null diags) do
    assertFailure . T.unpack $
      "compiling "
        <> T.pack colnFile
        <> " reported "
        <> T.pack (show (length diags))
        <> " diagnostic(s):\n"
        <> T.intercalate "\n" (map renderDiagnostic diags)
  pure
    [ (T.unpack n.last, lowerRealm n r)
    | (n, r) <- OMap.assocs globals.realms
    ]

renderDiagnostic :: Diagnostic ColnCode -> Text
renderDiagnostic = renderStrict . layoutPretty defaultLayoutOptions . dpretty

--------------------------------------------------------------------------------
-- Utilities
--------------------------------------------------------------------------------

jsonValue :: LBS.ByteString -> Aeson.Value
jsonValue = fromMaybe (error "invalid JSON") . Aeson.decode

prettyJson :: LBS.ByteString -> LBS.ByteString
prettyJson = Aeson.encodePretty . jsonValue

bwd :: [a] -> Bwd a
bwd = foldl (:>) BwdNil

path :: [Name] -> Path
path = bwd

tn :: Name -> [Name] -> TableName
tn realm p = TableName{realm, path = bwd p}

omap :: (Ord k) => [(k, v)] -> OMap.OMap k v
omap = OMap.fromList

--------------------------------------------------------------------------------
-- Hand-built FlatRealm values (for round-trip tests)
--
-- These cover IR constructs that the compiled .coln tests don't exercise: views
-- (all materializations), indexes, builtin column types, literal terms,
-- equality propositions, chased rules, multi-segment names, primary keys, and
-- the empty realm.  Which of those are still missing from the lowering pass is
-- recorded in test/outputs/lowered-coverage.txt.
--
-- Unlike the generated corpus, these are plausible theories rather than
-- arbitrary ones, and are worth reading; but they are examples only.  Don't
-- treat this list as the coverage story: that's "covers every constructor".
--------------------------------------------------------------------------------

singleTableTheory :: FlatRealm
singleTableTheory =
  FlatRealm
    { entities =
        omap
          [
            ( tn "Example" []
            , Entity
                { entityVariant = Table
                , columns = [(path ["x"], BuiltinTy BuiltinInt)]
                , primaryKey = Nothing
                }
            )
          ]
    , rules = omap []
    }

literalsTheory :: FlatRealm
literalsTheory =
  FlatRealm
    { entities =
        omap
          [
            ( tn "Constants" []
            , Entity
                { entityVariant = Table
                , columns =
                    [ (path ["i"], BuiltinTy BuiltinInt)
                    , (path ["s"], BuiltinTy BuiltinString)
                    ]
                , primaryKey = Nothing
                }
            )
          ]
    , rules =
        omap
          [
            ( tn "Constants" ["hasDefaults"]
            , Rule
                { ruleVariant = Chased
                , varNames = bwd []
                , varTypes = bwd []
                , antecedents = []
                , consequents =
                    [ PAtom
                        Atom
                          { entity = tn "Constants" []
                          , rowId = Nothing
                          , values =
                              omap
                                [ (0, Lit (LitInt 42))
                                , (1, Lit (LitString "hello"))
                                ]
                          }
                    ]
                }
            )
          ]
    }

allVariantsTheory :: FlatRealm
allVariantsTheory =
  FlatRealm
    { entities =
        omap
          [
            ( tn "T" []
            , Entity
                { entityVariant = Table
                , columns =
                    [ (path ["x"], BuiltinTy BuiltinInt)
                    , (path ["y"], BuiltinTy BuiltinInt)
                    ]
                , primaryKey = Just $ Set.fromList [path ["x"], path ["y"]]
                }
            )
          ,
            ( tn "T" ["recomputed"]
            , Entity
                { entityVariant = View Recomputed
                , columns = [(path ["x"], BuiltinTy BuiltinInt)]
                , primaryKey = Nothing
                }
            )
          ,
            ( tn "T" ["memoized"]
            , Entity
                { entityVariant = View Memoized
                , columns = [(path ["x"], BuiltinTy BuiltinInt)]
                , primaryKey = Nothing
                }
            )
          ,
            ( tn "T" ["materialized"]
            , Entity
                { entityVariant = View Materialized
                , columns = [(path ["x"], BuiltinTy BuiltinInt)]
                , primaryKey = Nothing
                }
            )
          ,
            ( tn "T" ["byX"]
            , Entity
                { entityVariant = Index BTree [path ["x"]]
                , columns = []
                , primaryKey = Nothing
                }
            )
          ]
    , rules =
        omap
          [
            ( tn "T" ["eqRule"]
            , Rule
                { ruleVariant = Enforced
                , varNames = bwd [path ["x"], path ["y"]]
                , varTypes = bwd [BuiltinTy BuiltinInt, BuiltinTy BuiltinInt]
                , antecedents =
                    [ PAtom
                        Atom
                          { entity = tn "T" []
                          , rowId = Nothing
                          , values = omap [(0, Var (FId 0)), (1, Var (FId 1))]
                          }
                    ]
                , consequents = [PEq (Var (FId 0)) (Var (FId 1))]
                }
            )
          ,
            ( tn "T" ["monitoredRule"]
            , Rule
                { ruleVariant = Monitored
                , varNames = bwd [path ["x"]]
                , varTypes = bwd [BuiltinTy BuiltinInt]
                , antecedents =
                    [ PAtom
                        Atom
                          { entity = tn "T" []
                          , rowId = Nothing
                          , values = omap [(0, Var (FId 0))]
                          }
                    ]
                , consequents = [PEq (Var (FId 0)) (Lit (LitInt 0))]
                }
            )
          ]
    }

multiSegNameTheory :: FlatRealm
multiSegNameTheory =
  FlatRealm
    { entities =
        omap
          [
            ( tn "Geometry" [Name ["shape"] "point"]
            , Entity
                { entityVariant = Table
                , columns =
                    [ (path [Name ["dimension"] "x"], BuiltinTy BuiltinInt)
                    , (path [Name ["dimension"] "y"], BuiltinTy BuiltinInt)
                    ]
                , primaryKey = Nothing
                }
            )
          ]
    , rules =
        omap
          [
            ( tn "Geometry" [Name ["shape"] "point", "positiveX"]
            , Rule
                { ruleVariant = Enforced
                , varNames = bwd [path [Name ["dimension"] "x"], path [Name ["dimension"] "y"]]
                , varTypes = bwd [BuiltinTy BuiltinInt, BuiltinTy BuiltinInt]
                , antecedents =
                    [ PAtom
                        Atom
                          { entity = tn "Geometry" [Name ["shape"] "point"]
                          , rowId = Nothing
                          , values = omap [(0, Var (FId 0)), (1, Var (FId 1))]
                          }
                    ]
                , consequents = [PEq (Var (FId 0)) (Lit (LitInt 5))]
                }
            )
          ]
    }
