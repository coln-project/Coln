-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Core.Layout where

import Data.Set (Set)
import Data.Set qualified as Set
import Data.String (fromString)
import Data.Vector.Strict qualified as Vector

import Coln.Common
import Coln.Core.Memoed qualified as M
import Coln.Core.Params
import Coln.Core.Readback
import Coln.Core.Realm
import Coln.Core.Syntax qualified as S
import Coln.Core.Value qualified as V

-- Layout is the process of creating a realm from a theory, along with the
-- universal model of that theory in the realm.

freshenBy :: Name -> String -> Name
freshenBy (Name qual last) s = Name (qual ++ [last]) (fromString s)

argName :: Set Name -> V.Clo a c -> Name
argName s (V.Clo x _ _) =
  head $ filter (\x -> not $ Set.member x s) (x : (freshenBy x <$> alphaStrings))
argName s (V.CloConst _) = head $ filter (\x -> not $ Set.member x s) alphaNames

data Scope = Scope
  { len :: CtxLen
  , names :: Bwd Name
  , ctx :: Bwd (S.Ty N)
  , bound :: Bwd (V.El N)
  , locals :: V.Locals
  , usedNames :: Set Name
  , realm :: RealmId
  }

emptyScope :: RealmId -> Scope
emptyScope = Scope 0 BwdNil BwdNil BwdNil V.LNil Set.empty

bind :: Scope -> Name -> V.Ty N -> (V.El N, Scope)
bind sc x a =
  let v = V.local (FId sc.len) a
      sc' = Scope (sc.len + 1) (sc.names :> x) (sc.ctx :> readb sc.len a) (sc.bound :> v) (V.LSnoc sc.locals v) (Set.insert x sc.usedNames) sc.realm
   in (v, sc')

layout :: Path -> Scope -> V.Ty N -> (Trie Generator, M.El N)
layout p sc a
  | (levelOf a).mlevel == Theory = case V.behavior a of
      V.LikeFunction ft -> do
        let x = argName sc.usedNames ft.cod
        let (v, sc') = bind sc x ft.dom
        let (gt, m) = layout p sc' (V.appClo ft.cod v)
        let m' = M.lam sc.locals (M.fromVTy sc.len ft.dom) (S.Abs x m)
        (gt, m')
      V.LikeRecord rt -> do
        let go _ [] = ([], [])
            go l ((x, a) : rest) = do
              let (gt, m) = layout (p :> x) sc (a l)
              let (gts, ms) = go (V.LSnoc l m.val) rest
              (gt : gts, m : ms)
        let (gts, ms) = go rt.capture (toList rt.fieldTypes)
        let m = M.cons (Dict rt.fieldTypes.head (Vector.fromList ms))
        (Node $ Dict rt.fieldTypes.head (Vector.fromList gts), m)
      V.LikeU (SetU; PropU) -> do
        -- TODO: layout Prop correctly
        let gt = Leaf (Rel (toList sc.names) (toList sc.ctx))
        let a = V.EltOf (TableName sc.realm p) (fromList $ zip (toList sc.names) (toList sc.bound))
        (gt, M.code (M.fromVTy sc.len a))
      V.NoRules -> panic "cannot layout type with no rules"
      V.LikeBuiltinTy _; V.LikeU _ -> panic "non-theory type"
  | (levelOf a).mlevel == Set = do
      let gt = Leaf (Fun (toList sc.names) (toList sc.ctx) (readb sc.len a))
      let v = V.Lookup (TableName sc.realm p) (fromList $ zip (toList sc.names) (toList sc.bound))
      (gt, M.fromVEl sc.len v)
  | otherwise = panic "tried to layout a toplevel type"

layoutTop :: RealmId -> V.Ty N -> (Trie Generator, M.El N)
layoutTop x = layout BwdNil (emptyScope x)
