-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.MIR.Layout where

import Data.Set qualified as Set
import Data.String (fromString)
import Data.Vector.Strict qualified as Vector

import Coln.Common

-- import Coln.Core.Globals
import Coln.Core.Params
import Coln.MIR.Memoed qualified as M
import Coln.MIR.Params
import Coln.MIR.Readback
import Coln.MIR.Realm
import Coln.MIR.Syntax qualified as S
import Coln.MIR.Value qualified as V

-- Layout is the process of creating a realm from a theory, along with the
-- universal model of that theory in the realm.

freshenBy :: Name -> String -> Name
freshenBy (Name qual last) s = Name (qual ++ [last]) (fromString s)

argName :: Set.Set Name -> V.Clo a b -> Name
argName _ (V.Clo x _) = x
argName used (V.CloConst _) = freshNameFor used

data Scope = Scope
  { len :: CtxLen
  , names :: Bwd Name
  , ctx :: Bwd (V.Ty Set)
  , bound :: Bwd (V.El Set)
  , locals :: V.Locals
  , usedNames :: Set.Set Name
  , realm :: RealmId
  }

emptyScope :: RealmId -> Scope
emptyScope = Scope 0 BwdNil BwdNil BwdNil BwdNil Set.empty

bind :: Scope -> Name -> V.Ty Set -> (V.El Set, Scope)
bind sc x a = do
  let v = V.local (FId sc.len)
      sc' =
        Scope
          { len = sc.len + 1
          , names = sc.names :> x
          , ctx = sc.ctx :> a
          , bound = sc.bound :> v
          , locals = sc.locals :> (Pair SSet v)
          , usedNames = Set.insert x sc.usedNames
          , realm = sc.realm
          }
  (v, sc')

args :: Scope -> [M.El Set]
args sc = [M.M (readb sc.len v) v | v <- toList sc.bound]

layout :: Path -> Scope -> V.Ty Theory -> (Trie Generator, M.El Theory)
layout p sc = \case
  V.LiftTy LSetTheory a -> do
    let gt = Leaf (Fun sc.names sc.ctx a)
    (gt, M.liftEl $ M.lookup (TableName sc.realm p) (args sc) (M.fromV sc.len a))
  V.U (inferSetCodes -> u) -> do
    let gt = Leaf (Rel u sc.names sc.ctx)
    (gt, M.code u $ M.eltOf (TableName sc.realm p) (args sc))
  V.Function ft -> case ft.variant of
    SSetTheory -> do
      let x = argName sc.usedNames ft.cod
      let (v, sc') = bind sc x ft.dom
      let (gt, m) = layout p sc' (V.appClo ft.cod v)
      (gt, M.lam sc.locals (M.fromV sc.len ft.dom) (S.Abs x m.stx))
  V.Record rt -> do
    let go _ [] = ([], [])
        go l ((x, a) : rest) = do
          let (gt, m) = layout (p :> x) sc (a l)
          let (gts, ms) = go (l :> Pair STheory m.val) rest
          (gt : gts, m : ms)
    let (gts, ms) = go rt.capture (toList rt.fieldTypes)
    let gt = Node $ Dict rt.fieldTypes.head (Vector.fromList gts)
    (gt, M.cons (Dict rt.fieldTypes.head (Vector.fromList ms)))

layoutTop :: RealmId -> V.Ty Theory -> (Trie Generator, M.El Theory)
layoutTop x = layout BwdNil (emptyScope x)
