-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.SIR.Cache where

import Coln.Common
import Coln.Core.Params
import Coln.MIR.Memoed qualified as M
import Coln.MIR.Params
import Coln.MIR.Realm qualified as V
import Coln.MIR.Value qualified as V
import Coln.SIR.Realm
import Coln.SIR.Separate
import Coln.SIR.Syntax qualified as S


import Control.Monad (forM)
import Control.Monad.Reader.Class (local)
import Data.Set qualified as Set

data Scope = Scope
  { len :: Int
  , ctx :: Bwd S.Query
  , names :: Bwd Name
  , bound :: Bwd (V.El N Set)
  , used :: Set.Set Name
  }

emptyScope :: Scope
emptyScope =
  Scope
    { len = 0
    , ctx = BwdNil
    , names = BwdNil
    , bound = BwdNil
    , used = Set.empty
    }

bind :: Scope -> Maybe Name -> V.Ty N Set -> (Name, V.El N Set, Scope)
bind sc mx a = do
  let q = separate sc.len a
  let x = case mx of
        Just x -> freshenFor sc.used x
        Nothing -> freshNameFor sc.used
  let v = V.local (FId sc.len)
  let sc' =
        sc
          { len = sc.len + 1
          , ctx = sc.ctx :> q
          , names = sc.names :> x
          , bound = sc.bound :> v
          , used = Set.insert x sc.used
          }
  (x, v, sc')

emptyNode :: Trie a
emptyNode = Node $ fromList []

cloArgName :: V.Clo a b -> Maybe Name
cloArgName (V.Clo x _) = Just x
cloArgName (V.CloConst _) = Nothing

-- TODO: replace this Path with the one in the Reader for GeneratorWriter
cache :: Name -> Path -> Scope -> V.El N Theory -> GeneratorWriter (S.El Theory)
cache x p sc v = do
  let code u a = do
        -- let sa = separate sc.len a
        -- let x' = case Set.member x sc.used of
        --       True -> freshNameFor sc.used
        --       False -> x
        -- let cols = zip (toList $ sc.names :> x') (toList $ sc.ctx :> sa)
        -- let bound = toList (sc.bound :> V.local (FId sc.len))
        -- let boundStx = separate (sc.len + 1) <$> bound
        -- emitTable $ Table (View Materialized) (second (.shape) <$> cols) (Just [0 .. sc.len])
        let tn = manglePath p
        -- local (:> "definition") $ emitRule $ Rule cols tn boundStx
        pure $ S.SelectLast u tn (separate sc.len <$> toList sc.bound) (shapeOf a)
  case v of
    V.LiftEl LSetTheory v -> pure $ S.LiftEl (separate sc.len v)
    V.Code SSetU a -> code SSetU a
    V.Code SPropU a -> code SPropU a
    V.PrimCode _ u tn args -> pure $ S.SelectRowId u tn (separate sc.len <$> args)
    V.Lam SSetTheory dom clo -> do
      let (x', arg, sc') = bind sc (cloArgName clo) dom
      body <- cache x p sc' (V.appClo clo arg)
      pure $ S.Lam (separate sc.len dom) (S.Abs (Just x') body)
    V.Bundle fields -> do
      fields' <- forM (toList fields) $ \(x', field) -> local (:> x') $ do
        cache x' (p :> x') sc field
      pure $ S.Bundle (Dict fields.head (fromList fields'))

cacheTop :: Name -> V.RealmDefinition -> GeneratorWriter (S.El Theory)
cacheTop x def = cache x (BwdNil :> "view" :> x) emptyScope def.body.val
