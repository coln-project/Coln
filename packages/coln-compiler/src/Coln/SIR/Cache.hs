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

import Control.Arrow (second)
import Data.Set qualified as Set

data Scope = Scope
  { len :: Int
  , ctx :: Bwd S.Query
  , names :: Bwd Name
  , bound :: Bwd (V.El N Set)
  , used :: Set.Set Name
  , realm :: RealmId
  }

emptyScope :: RealmId -> Scope
emptyScope rId =
  Scope
    { len = 0
    , ctx = BwdNil
    , names = BwdNil
    , bound = BwdNil
    , used = Set.empty
    , realm = rId
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

cache :: Name -> Path -> Scope -> V.El N Theory -> (Trie Entity, Trie Definition, S.El Theory)
cache x p sc v = do
  let code u a = do
        let sa = separate sc.len a
        let x' = case Set.member x sc.used of
              True -> freshNameFor sc.used
              False -> x
        let cols = zip (toList $ sc.names :> x') (toList $ sc.ctx :> sa)
        let bound = toList (sc.bound :> V.local (FId sc.len))
        let boundStx = separate (sc.len + 1) <$> bound
        let ent = Entity (View Materialized) (second (.shape) <$> cols) (Just [0 .. sc.len])
        let tn = TableName sc.realm p
        let def = Definition cols tn boundStx
        let prop = S.Atom tn S.Erased boundStx
        let elt = S.Multi u $ S.Query sa.shape (S.Abs Nothing prop)
        (Leaf ent, Node (fromList [("definition", Leaf def)]), elt)
  case v of
    V.LiftEl LSetTheory v -> (emptyNode, emptyNode, S.LiftEl (separate sc.len v))
    V.Code SSetU a -> code SSetU a
    V.Code SPropU a -> code SPropU a
    V.Lam SSetTheory dom clo -> do
      let (x', arg, sc') = bind sc (cloArgName clo) dom
      let (ents, defs, body) = cache x p sc' (V.appClo clo arg)
      (ents, defs, S.Lam (separate sc.len dom) (S.Abs (Just x') body))
    V.Cons fields -> do
      let (ents, defs, fields') =
            unzip3 [cache x' (p :> x') sc field | (x', field) <- toList fields]
      ( Node (Dict fields.head (fromList ents))
        , Node (Dict fields.head (fromList defs))
        , S.Cons (Dict fields.head (fromList fields'))
        )

cacheTop :: RealmId -> Name -> V.RealmDefinition -> (Trie Entity, Trie Definition, S.El Theory)
cacheTop rId x def = cache x (BwdNil :> x) (emptyScope rId) def.body.val
