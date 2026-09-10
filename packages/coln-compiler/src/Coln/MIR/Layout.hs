-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.MIR.Layout where

import Data.Set qualified as Set
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

argName :: Set.Set Name -> V.Clo a b -> Name
argName used (V.Clo x _) = freshenFor used x
argName used (V.CloConst _) = freshNameFor used

data Scope = Scope
  { len :: CtxLen
  , names :: Bwd Name
  , ctx :: Bwd (V.Ty N Set)
  , bound :: Bwd (V.El N Set)
  , locals :: V.Locals
  , usedNames :: Set.Set Name
  , realm :: RealmId
  }

emptyScope :: RealmId -> Scope
emptyScope = Scope 0 BwdNil BwdNil BwdNil BwdNil Set.empty

bind :: Scope -> Name -> V.Ty N Set -> (V.El N Set, Scope)
bind sc x a = do
  let v = V.local (FId sc.len)
      sc' =
        sc
          { len = sc.len + 1
          , names = sc.names :> x
          , ctx = sc.ctx :> a
          , bound = sc.bound :> v
          , locals = sc.locals :> Pair SSet v
          , usedNames = Set.insert x sc.usedNames
          }
  (v, sc')

args :: Scope -> [M.El N Set]
args sc = [M.M (readb sc.len v) v | v <- toList sc.bound]

layout :: Path -> Providence -> Scope -> V.Ty N Theory -> (Trie Generator, M.El N Theory)
layout p pr sc = \case
  V.LiftTy LSetTheory a -> do
    let gt = Leaf (Generator pr sc.names sc.ctx (GenLift a))
    (gt, M.liftEl $ M.lookup (TableName sc.realm p) (args sc) (M.fromV sc.len a))
  V.U (inferSetCodes -> u) -> do
    let gt = Leaf (Generator pr sc.names sc.ctx (GenU u))
    (gt, M.primCode u (TableName sc.realm p) (args sc))
  V.Function ft -> case ft.variant.mlevel of
    SSetTheory -> do
      let x = argName sc.usedNames ft.cod
      let (v, sc') = bind sc x ft.dom
      let (gt, m) = layout p pr sc' (V.appClo ft.cod v)
      (gt, M.lam sc.locals (M.fromV sc.len ft.dom) (S.Abs x m.stx))
  V.Record rt -> do
    let go _ [] = ([], [])
        go l ((x, a) : rest) = do
          let (gt, m) = layout (p :> x) pr sc (a l)
          let (gts, ms) = go (l :> Pair STheory m.val) rest
          (gt : gts, m : ms)
    let (gts, ms) = go rt.capture (toList rt.fieldTypes)
    let gt = Node $ Dict rt.fieldTypes.head (Vector.fromList gts)
    (gt, M.cons (Dict rt.fieldTypes.head (Vector.fromList ms)))

layoutTop :: RealmId -> V.Ty N Theory -> (Trie Generator, M.El N Theory)
layoutTop x = layout (BwdNil :> "root") Profane (emptyScope x)

asNominative :: V.El D Set -> V.El N Set
asNominative = \case
  V.Cons fields -> V.Cons $ flip fmap fields $ \case
    V.Become v -> v
    V.Describe v -> asNominative v

emptyNode :: Trie Generator
emptyNode = Node $ fromList []

declareEvaluation :: Path -> Scope -> V.Evaluation V.El D Theory -> (Trie Generator, M.El N Theory)
declareEvaluation p sc = \case
  V.Become v -> (emptyNode, M.fromV sc.len v)
  V.Describe v -> declare p sc v

declare :: Path -> Scope -> V.El D Theory -> (Trie Generator, M.El N Theory)
declare p sc = \case
  V.LiftEl LSetTheory v -> (emptyNode, M.liftEl $ M.fromV sc.len $ asNominative v)
  V.Lam SSetTheory dom clo -> do
    let x = argName sc.usedNames clo
    let (v, sc') = bind sc x dom
    let (gt, m) = declareEvaluation p sc' (V.appClo clo v)
    (gt, M.lam sc.locals (M.fromV sc.len dom) (S.Abs x m.stx))
  V.Cons fields -> do
    let (gts, ms) = unzip $ for (toList fields) $ \(x, v) ->
          declareEvaluation (p :> x) sc v
    (Node $ withHead fields gts, M.cons $ withHead fields ms)
  V.Init a -> layout p Holy sc a
