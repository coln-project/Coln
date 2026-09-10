-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT
{-# OPTIONS_GHC -Wno-orphans #-}

module Coln.Core.Print where

import Data.Map.Ordered qualified as OMap

import Coln.Common
import Coln.Core.Globals
import Coln.Core.Memoed (Memoed (..))
import Coln.Core.Params
import Coln.Core.Readback
import Coln.Core.Syntax
import Coln.Frontend.Notation
import Data.List.NonEmpty (NonEmpty (..))
import Data.String (fromString)
import Data.Text qualified as T
import FNotation qualified as N

-- Pretty printing
--------------------------------------------------------------------------------

type Names = Bwd Name

class ToNotation a where
  toNotation :: Names -> a -> N.Ntn0

class ToNotationTop a where
  toNotationTop :: a -> N.Ntn0

instance ToNotation BId where
  toNotation xs (BId i) = go xs i []
   where
    go (_ :> x) 0 prev = N.Ident (x{last = x.last <> disamb}) ()
     where
      nx = length $ filter (== x) prev
      disamb = if nx > 0 then "^" <> T.pack (show nx) else ""
    go (xs' :> x) n prev = go xs' (n - 1) (x : prev)
    go BwdNil _ _ = error $ "name " ++ show i ++ " not bound. ?names = " ++ (show $ toList xs)

instance ToNotation TableName where
  toNotation _ x = N.Group (N.Ident x.realm () :| [N.Field s () | s <- toList x.path])

instance ToNotation (El e) where
  toNotation xs = \case
    LocalVar i -> toNotation xs i
    GlobalVar x _ -> N.Ident x ()
    Code _ ty -> toNotation xs ty
    App _ f t -> N.Juxt (toNotation xs f) (toNotation xs t)
    Lam _ _ (Abs x t) ->
      N.Infix (N.Ident x ()) (N.Keyword "=>" ()) (toNotation (xs :> x) t)
    Lam _ _ (AbsConst t) ->
      N.Infix (N.Ident "_" ()) (N.Keyword "=>" ()) (toNotation xs t)
    Proj _ t f -> N.Juxt (toNotation xs t) (N.Field f ())
    Cons _ d ->
      N.Tuple [field y t | (y, t) <- toList d] ()
     where
      field y t = N.Infix (N.Ident y ()) (N.Keyword ":=" ()) (toNotation xs t)
    Init t -> N.Juxt (N.Keyword "init" ()) (toNotation xs t)
    Lit (LitInt i) -> N.Int i ()
    Lit (LitString s) -> N.String s ()
    Is t -> toNotation xs t -- invisible

nbinding :: Name -> N.Ntn0 -> N.Ntn0
nbinding x n = N.Infix (N.Ident x ()) (N.Keyword ":" ()) n

instance ToNotation (Ty e) where
  toNotation xs = \case
    U u -> N.Keyword (fromString $ show $ pretty u) ()
    Decode _ t -> toNotation xs t
    Function f -> case f.cod of
      Abs x b ->
        N.Infix
          (nbinding x (toNotation xs f.dom))
          (N.Keyword "->" ())
          (toNotation (xs :> x) b)
      AbsConst b ->
        N.Infix
          (toNotation xs f.dom)
          (N.Keyword "->" ())
          (toNotation xs b)
    Record r -> N.Block "sig" Nothing (go xs $ toList r.fieldTypes) ()
     where
      go _ [] = []
      go xs' ((y, a) : pairs') =
        nbinding y (toNotation xs' a) : go (xs' :> y) pairs'
    Eq eq ->
      N.Infix
        (toNotation xs eq.lhs)
        (N.Keyword "=" ())
        (toNotation xs eq.rhs)
    BuiltinTy a -> N.Keyword (fromString $ show a) ()
    IsTy a -> toNotation xs a

instance ToNotation TypeBehavior where
  toNotation xs = \case
    LikeU u -> toNotation xs (U u)
    LikeFunction ft -> toNotation xs (Function ft)
    LikeRecord rt -> toNotation xs (Record rt)
    LikeBuiltinTy bt -> toNotation xs (BuiltinTy bt)
    LikeInductive t -> toNotation xs t
    NoRules -> N.Keyword "NoRules" ()

toNotationTele :: [Name] -> [Ty N] -> [N.Ntn0]
toNotationTele xs tys = go xs BwdNil tys
 where
  go _ _ [] = []
  go (x : xs') names (a : tys') = do
    let bnd = (N.Infix (N.Ident x ()) (N.Keyword ":" ()) (toNotation names a))
    bnd : go xs' (names :> x) tys'
  go _ _ _ = panic "mismatched lengths"

-- instance ToNotationTop Generator where
--   toNotationTop (Rel xs tys) =
--     N.Juxt (N.Keyword "rel" ()) (N.Tuple (toNotationTele xs tys) ())
--   toNotationTop (Fun xs tys ret) =
--     N.Infix
--       (N.Juxt (N.Keyword "fun" ()) (N.Tuple (toNotationTele xs tys) ()))
--       (N.Keyword "->" ())
--       (toNotation (fromList xs) ret)

instance (ToNotationTop a) => ToNotationTop (Trie a) where
  toNotationTop (Leaf g) = toNotationTop g
  toNotationTop (Node d) =
    N.Block
      "node"
      Nothing
      [N.Infix (N.Ident x ()) (N.Keyword "=" ()) (toNotationTop t) | (x, t) <- toList d]
      ()

instance ToNotationTop Realm where
  toNotationTop r =
    N.Block
      "realm"
      Nothing
      (go (BwdNil :> "root") (OMap.assocs r.realmDefinitions))
      ()
   where
    toAnn Conjunctive = []
    toAnn Inductive = ["ind"]
    go _ [] = []
    go xs ((x, d) : ds) = do
      let body = toNotation xs d.body.stx
      let ty = toNotation xs $ readb (length xs) d.ty
      let head = N.Infix (N.Ident x ()) (N.Keyword ":" ()) ty
      let def = N.MDecl (toAnn d.mode) "def" (N.Infix head (N.Keyword ":=" ()) body) ()
      def : go (xs :> x) ds

-- DPrettyWithNames
--------------------------------------------------------------------------------

class DPrettyWithNames a where
  dprettyWithNames :: Names -> a -> DDoc

instance DPrettyWithNames BId where
  dprettyWithNames xs t = N.dprettyWithConfigs readConfig lexConfig $ toNotation xs t

instance DPrettyWithNames (El e) where
  dprettyWithNames xs t = N.dprettyWithConfigs readConfig lexConfig $ toNotation xs t

instance DPrettyWithNames (Ty e) where
  dprettyWithNames xs t = N.dprettyWithConfigs readConfig lexConfig $ toNotation xs t

instance DPrettyWithNames TypeBehavior where
  dprettyWithNames xs t = N.dprettyWithConfigs readConfig lexConfig $ toNotation xs t

instance DPretty Realm where
  dpretty r = N.dprettyWithConfigs readConfig lexConfig $ toNotationTop r

class HasShape a where
  shape :: a -> CtxShape

instance HasShape CtxShape where
  shape = id

prtIn :: (HasShape c, Readback a b, DPrettyWithNames b) => c -> a -> DDoc
prtIn c v = do
  let cs = shape c
  dprettyWithNames cs.names $ readb cs.len v
