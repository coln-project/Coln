-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.MIR.Top where

import Control.Arrow ((***))
import Data.Map.Ordered qualified as OMap
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

evalToNominative :: V.Description V.El l -> V.El N l
evalToNominative = \case
  V.Describe v -> toNominative v
  V.Become v -> v

toNominative :: V.El D l -> V.El N l
toNominative = \case
  V.LiftEl l v -> V.LiftEl l (toNominative v)
  V.Init _ -> panic "init not allowed in globals"
  V.Lam fv a clo -> do
    let clo' = case clo of
         V.Clo x f -> V.Clo x (evalToNominative . f)
         V.CloConst v -> V.CloConst (evalToNominative v)
    V.Lam fv a clo'
  V.Cons fields -> V.Cons $ evalToNominative <$> fields

interpGlobals :: Core.Globals -> V.Globals
interpGlobals g = foldl go OMap.empty $ OMap.assocs g.definitions
 where
  interp' :: V.Globals -> Name -> Core.Definition Global -> Match SMLevel (V.El N)
  interp' acc _ def = case interp acc BwdNil def.body.stx of 
    Pair l v -> Pair l (evalToNominative v)
  go :: V.Globals -> (Name, Core.Definition Global) -> V.Globals
  go acc (x, def) = acc OMap.>| (x, interp' acc x def)

coreToMIR :: V.Globals -> RealmId -> Core.Realm -> MIR.Realm
coreToMIR g rId r = do
  let rTy = interpAt STheory g BwdNil r.rootType.stx
  let (rootgens, rootbody) = layoutTop rId rTy
  let go :: (Int, V.Locals) -> (Name, Core.Definition Local) -> ((Int, V.Locals), (Name, (Trie Generator, RealmDefinition)))
      go (n, ls) (x, def) = do
        let ty = interpAt STheory g ls $ readb n def.ty
        let bodyD = interpAt STheory g ls def.body.stx
        let (gens, body) = declareEvaluation (BwdNil :> "init" :> x) (emptyScope rId) bodyD
        let l' = Pair STheory body.val
        let def' =
              RealmDefinition
                { body = body
                , ty = ty
                }
        ((n + 1, ls :> l'), (x, (gens, def')))
  let (gens, defs) = fromList *** OMap.fromList $ unzip $ fmap (\(x,(y,z)) -> ((x,y),(x,z))) $ snd $ mapAccumL go (1, BwdNil :> Pair STheory rootbody.val) $ OMap.assocs r.realmDefinitions
  MIR.Realm
    { root = rootbody.val
    , rootType = r.rootType.val
    , generators = Node $ fromList $ [("root", rootgens), ("init", Node gens)]
    , realmDefinitions = defs
    }
