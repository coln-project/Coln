-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Elaborator.Environment where

import Coln.Common
import Coln.Core
import Coln.Core.Value qualified as BN (BareNeutral (..))
import Coln.Core.Value qualified as V
import Coln.Elaborator.Diagnostics
import Data.Vector.Strict qualified as Vector

-- * Scope

data Scope = Scope
  { len :: Int
  , names :: Bwd Name
  , locals :: V.Locals
  , ctx :: Bwd (V.Ty N)
  }

emptyScope :: Scope
emptyScope = Scope 0 BwdNil V.LNil BwdNil

instance HasShape Scope where
  shape :: Scope -> CtxShape
  shape c = CtxShape c.len c.names

bind :: Name -> V.Ty N -> Scope -> Scope
bind x a c = do
  let v = V.local (FId c.len) a
  let_ x v a c

let_ :: Name -> V.El N -> V.Ty N -> Scope -> Scope
let_ x v a c =
  Scope
    (c.len + 1)
    (c.names :> x)
    (V.LSnoc c.locals v)
    (c.ctx :> a)

withBound :: Name -> V.Ty N -> Scope -> (V.El N -> Scope -> a) -> a
withBound x a c body = do
  let v = V.local (FId c.len) a
  let c' = let_ x v a c
  body v c'

instance Lookup Scope Name (BId, V.El N, V.Ty N) where
  lookup sc x = go sc.len sc.len sc.names sc.locals sc.ctx 0
   where
    go 0 0 BwdNil V.LNil BwdNil _ = Nothing
    go 0 0 BwdNil (V.LSnocChunk vs chunk) BwdNil i
      | Vector.length chunk == 0 = go 0 0 BwdNil vs BwdNil i
    go n _ (xs :> x') (V.LSnoc vs v) (ts :> t) i
      | x' == x = Just (BId i, v, t)
      | otherwise = go (n - 1) (n - 1) xs vs ts (i + 1)
    go n m xs@(xs' :> x') vs@(V.LSnocChunk vs' chunk) ts@(ts' :> t) i
      | m - n == Vector.length chunk = go n n xs vs' ts i
      | x' == x = Just (BId i, chunk Vector.! (Vector.length chunk + n - m - 1), t)
      | otherwise = go n m xs' vs ts' (i + 1)
    go _ _ _ _ _ _ = panic "misaligned local variable details"

-- * Target

data Target :: Case -> Type where
  TargetAnonymous :: Target N
  TargetNamed :: V.BareNeutral -> Target D

projTarget :: Target c -> Name -> Target c
projTarget TargetAnonymous _ = TargetAnonymous
projTarget (TargetNamed n) x = TargetNamed n{BN.spine = V.Proj n.spine x}

appTarget :: Target c -> V.El N -> Target c
appTarget TargetAnonymous _ = TargetAnonymous
appTarget (TargetNamed n) x = TargetNamed n{BN.spine = V.App n.spine x}

reflectTarget :: Target c -> V.Ty N -> V.Evaluation V.El c -> V.El N
reflectTarget TargetAnonymous _ v = v
reflectTarget (TargetNamed n) t v = V.reflect n.head n.spine t (Just v)

-- * Full environment

data ElabEnv c = ElabEnv
  { target :: Target c
  , scope :: Scope
  , globals :: Globals
  , diagEnv :: DiagnosticEnv ElaboratorCode
  }

instance HasShape (ElabEnv c) where
  shape e = shape e.scope

emptyElabEnvFor :: DiagnosticEnv ElaboratorCode -> Globals -> Name -> V.Ty N -> ElabEnv D
emptyElabEnvFor diagEnv globals x ty = do
  let v = V.reflect (V.GlobalVar x v) V.Id ty Nothing
  ElabEnv
    { target = (TargetNamed (V.BareNeutral (V.GlobalVar x v) V.Id))
    , scope = emptyScope
    , globals = globals
    , diagEnv = diagEnv
    }

emptyElabEnv :: DiagnosticEnv ElaboratorCode -> Globals -> ElabEnv N
emptyElabEnv diagEnv globals =
  ElabEnv
    { target = TargetAnonymous
    , scope = emptyScope
    , globals = globals
    , diagEnv = diagEnv
    }
