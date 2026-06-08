module Coln.Core.Layout where

import Data.String (fromString)
import Data.Set qualified as Set
import Data.Set (Set)
import Data.Vector.Strict qualified as Vector

import Coln.Common
import Coln.Core.Params
import Coln.Core.Readback
import Coln.Core.Value qualified as V
import Coln.Core.Syntax qualified as S
import Coln.Core.Memoed qualified as M
import Coln.Core.Realm

-- Layout is the process of creating a realm from a theory, along with the
-- universal model of that theory in the realm.

namesOfLen :: Int -> [String]
namesOfLen 0 = [""]
namesOfLen n = [c : cs | c <- ['a' .. 'z'], cs <- namesOfLen (n - 1)]

names :: [Name]
names = fromString <$> go 1
  where
    go n = namesOfLen n ++ go (n + 1)

argName :: Set Name -> V.Clo a c -> Name
argName _ (V.Clo x _ _) = x
argName s (V.CloConst _) = head $ filter (\x -> not $ Set.member x s) names

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

layout :: Path -> Scope -> V.Ty N -> (GenTrie, M.El N)
layout p sc a
  | levelOf a == Theory = case V.behavior a of
      V.LikeFunction ft -> do
        let x = argName sc.usedNames ft.cod
        let (v, sc') = bind sc x a
        let (gt, m) = layout p sc' (V.appClo ft.cod v)
        let m' = M.lam sc.locals (M.fromVTy sc.len ft.dom) (S.Abs x m)
        (gt, m')
      V.LikeRecord rt -> do
        let go _ [] = ([], [])
            go l ((x, a):rest) = do
              let (gt, m) = layout (p :> x) sc (a l)
              let (gts, ms) = go (V.LSnoc l m.val) rest
              (gt : gts, m : ms)
        let (gts, ms) = go rt.capture (toList rt.fieldTypes)
        let m = M.cons (Dict rt.fieldTypes.head (Vector.fromList ms))
        (Node $ Dict rt.fieldTypes.head (Vector.fromList gts), m)
      V.LikeU SetU -> do
        let gt = Leaf (Rel (toList sc.names) (toList sc.ctx))
        let a = V.EltOf (TableName sc.realm p) (toList sc.bound)
        (gt, M.code (M.fromVTy sc.len a))
      V.NoRules -> panic "cannot layout type with no rules"
      V.LikeBuiltinTy _; V.LikeU _ -> panic "non-theory type"
  | levelOf a == Set = do
      let gt = Leaf (Fun (toList sc.names) (toList sc.ctx) (readb sc.len a))
      let v = V.Lookup (TableName sc.realm p) (toList sc.bound)
      (gt, M.fromVEl sc.len v)
  | otherwise = panic "tried to layout a toplevel type"
