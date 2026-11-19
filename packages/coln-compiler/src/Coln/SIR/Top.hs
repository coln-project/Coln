-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.SIR.Top where


import Data.Key (forWithKey_, forWithKeyM)
import Data.Map.Ordered qualified as OMap

import Control.Monad (forM)
import Control.Monad.Reader (local, ask)

import Coln.Common
import Coln.Core.Params
import Coln.MIR.Memoed (Memoed (..))
import Coln.MIR.Realm qualified as MIR
import Coln.SIR.Cache
import Coln.SIR.Realm qualified as SIR
import Coln.SIR.Separate

locals :: Trie a -> (a -> SIR.GeneratorWriter ()) -> SIR.GeneratorWriter ()
locals (Leaf x) f = f x
locals (Node d) f = forWithKey_ d (\x t -> local (:> x) (locals t f))

mirToSIR :: MIR.Realm -> SIR.Realm
mirToSIR r = do
  let (auxillaries, generators) = SIR.runGeneratorWriter $ do
        root <- cache "root" (BwdNil :> "root") emptyScope r.root
        locals r.generators separateGenerator
        let rootTy = theoryShapeOf (CtxShape 0 BwdNil) r.rootType r.root
        fmap (OMap.fromList . (("root", (root, rootTy)):)) $ local (:> "view") $ forM (OMap.assocs r.realmDefinitions) $ \(x, d) -> local (:> x) $ do
          el <- cacheTop x d
          let ty = theoryShapeOf (CtxShape 0 BwdNil) d.ty d.body.val
          pure (x, (el, ty))
  SIR.Realm generators auxillaries
        

-- mirToSIR :: MIR.Realm -> SIR.Realm
-- mirToSIR r = do
--   let (_, _, root) = cache "root" (BwdNil :> "root") emptyScope r.root
--   let (rootE, rootD, rootR) = aggregate3 (\p -> separateGenerator (manglePath p)) BwdNil r.generators
--   let (names, cached) = unzip $ map (fst &&& uncurry cacheTop) $ OMap.assocs r.realmDefinitions
--   let auxTypes = map (\(_, d) -> theoryShapeOf (CtxShape 0 BwdNil) d.ty d.body.val) $ OMap.assocs r.realmDefinitions
--   let (cachedE, cachedD, cachedS) = unzip3 cached
--   let viewE = Node $ fromList [(x, y) | (x, Just y) <- zip names (map cleanTrie cachedE)]
--   let viewD = Node $ fromList [(x, y) | (x, Just y) <- zip names (map cleanTrie cachedD)]
--   let generators = SIR.Generators
--         { tables = trieToOMap $ Node $ fromList $ fromNode rootE ++ [("view", viewE)]
--         , unions = trieToOMap $ Node $ fromList []
--         , rules = trieToOMap $ Node $ fromList $ fromNode rootD ++ [("view", viewD)]
--         , laws = trieToOMap $ fromMaybe emptyNode rootR
--         }
--   SIR.Realm
--     { generators = generators
--     , auxillaries = OMap.fromList $ zip names (zip cachedS auxTypes)
--     }
