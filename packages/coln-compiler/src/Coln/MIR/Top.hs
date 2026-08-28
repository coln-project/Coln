-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.MIR.Top where

import Data.Traversable (mapAccumL)

import Coln.Common
import Coln.Core.Globals qualified as Core
import Coln.Core.Memoed qualified as Core
import Coln.Core.Params
import Coln.Core.Readback
import Coln.MIR.Interpret
import Coln.MIR.Layout
import Coln.MIR.Memoed qualified as M
import Coln.MIR.Params (SMLevel (..))
import Coln.MIR.Realm as MIR
import Coln.MIR.Value qualified as V

coreToMIR :: V.Globals -> RealmId -> Core.Realm -> MIR.Realm
coreToMIR g rId r = do
  let rTy = interpAt STheory g BwdNil r.rootType.stx
  let (gens, root) = layoutTop rId rTy
  let go :: (Int, V.Locals) -> Core.Definition Local -> ((Int, V.Locals), RealmDefinition)
      go (n, ls) def = do
        let ty = interpAt STheory g ls $ readb n def.ty
        let body = interpAt STheory g ls def.body.stx
        let l' = Pair STheory body
        let def' =
              RealmDefinition
                { body = M.fromV n body
                , ty = ty
                }
        ((n + 1, ls :> l'), def')
  let (_, defs) = mapAccumL go (1, BwdNil :> Pair STheory root.val) r.realmDefinitions
  MIR.Realm
    { root = root.val
    , rootType = r.rootType.val
    , generators = gens
    , realmDefinitions = defs
    }
