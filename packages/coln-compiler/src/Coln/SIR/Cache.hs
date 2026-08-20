module Coln.SIR.Cache where

import Coln.Common
import Coln.Core.Params
import Coln.MIR.Params
import Coln.MIR.Value qualified as V
import Coln.SIR.Realm
import Coln.SIR.Syntax qualified as S
import Coln.SIR.Separate

import Data.Set qualified as Set

data Scope = Scope
  { len :: Int
  , ctx :: Bwd S.Query
  , names :: Bwd Name
  , bound :: Bwd (V.El Set)
  , used :: Set.Set Name
  , realm :: RealmId
  }

bind :: Scope -> Maybe Name -> V.Ty Set -> (Name, V.El Set, Scope)
bind sc mx a = do
  let q = separate sc.len a
  let x = case mx of
        Just x -> x
        Nothing -> freshNameFor sc.used
  let v = V.local (FId sc.len)
  let sc' = sc
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

cache :: Path -> Scope -> V.El Theory -> (Trie Entity, Trie Definition, S.El Theory)
cache p sc v = do
  let code u a = do
        let sa = separate sc.len a
        let cols = toList (sc.ctx :> sa)
        let bound = toList (sc.bound :> V.local (FId sc.len))
        let boundStx = separate (sc.len + 1) <$> bound
        let ent = Entity View (toList sc.names) ((.shape) <$> cols) (Just [0..sc.len])
        let tn = TableName sc.realm p
        let def = Definition cols tn boundStx 
        let prop = S.Atom tn Nothing boundStx
        let elt = S.Multi u $ S.Query sa.shape (S.Abs Nothing prop)
        (Leaf ent, Node (fromList [("definition", Leaf def)]), elt)
  case v of
    V.LiftEl LSetTheory v -> (emptyNode, emptyNode, S.LiftEl (separate sc.len v))
    V.Code SSetU a -> code SSetU a
    V.Code SPropU a -> code SSetU a
    V.Lam SSetTheory dom clo -> do
      let (x, arg, sc') = bind sc (cloArgName clo) dom
      let (ents, defs, body) = cache p sc' (V.appClo clo arg)
      (ents, defs, S.Lam (separate sc.len dom) (S.Abs (Just x) body))
    V.Cons fields -> do
      let (ents, defs, fields') =
            unzip3 [ cache (p :> x) sc field | (x, field) <- toList fields ]
      ( Node (Dict fields.head (fromList ents))
        , Node (Dict fields.head (fromList defs))
        , S.Cons (Dict fields.head (fromList fields'))
        )
