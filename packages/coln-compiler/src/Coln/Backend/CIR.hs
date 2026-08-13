-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Backend.CIR where

import Data.Map.Ordered (OMap)

import Coln.Common
import Coln.Backend.FLIR qualified as FLIR
import Coln.Core
import Coln.Core.Value qualified as V

-- This will be evaluated at runtime to produce a FLIR Term
data Term
  = HostVar BId [Name]
  | Lit Literal
  | QueryVar FId

-- This will be evaluated at runtime to produce a FLIR Atom
data Atom = Atom
  { entity :: TableName
  , rowId :: Maybe Term
  , values :: OMap Int Term
  }

-- This will be evaluated at runtime to produce a FLIR Prop
data Prop
  = PEq Term Term
  | PAtom Atom

-- This will be evaluated at runtime to produce a FLIR Query
data Query = Query
  { variables :: [FLIR.ColType]
  , predicates :: [Prop]
  , reconstruction :: Clo -- expects a tuple as an argument
  }

data Clo = Clo Name El

data El
  = Var BId
  | Index El FId
  | All Query -- Return all results of this query
  | Only Query -- Return the unique result of this query
  | Cons (Dict El)
  | Proj El Name
  | Lam Clo
  | App El El

compileSpine :: V.Spine -> El -> El
compileSpine = \case
  V.Id -> \t -> t
  V.Proj sp x -> \t -> Proj (compileSpine sp t) x
  V.App _ _ -> panic "compilation expects no large neutrals"

compileHead :: Int -> V.Head -> El
compileHead n = \case
  V.LocalVar (FId i) -> Var $ BId $ n - i - 1
  V.GlobalVar _ _ -> panic "compilation expects no large neutrals"
  V.Lookup x args _ -> 

compileNe :: Int -> V.Neutral -> El
compileNe n ne = compileSpine ne.spine $ compileHead n ne.head

compile :: Int -> V.El N -> El
compile n = \case
  V.Neu ne -> compileNe n ne
  _ -> undefined

-- How does this interact with lowering to FLIR? There's clearly some
-- duplication here.

-- Certainly, the `Term` and `Pred` structure in `Lower.hs` are relevant; we
-- need to use these and then use stuff like pushTerm...
