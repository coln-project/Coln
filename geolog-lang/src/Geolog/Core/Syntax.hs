module Geolog.Core.Syntax where

import Data.Kind (Type)
import Data.Map qualified as Map

import Geolog.Common
import Geolog.Core.Params
import Geolog.Core.Value qualified as V

-- Abstractions
--------------------------------------------------------------------------------

data Abs (f :: Case -> Type) (c :: Case) = Abs Name (f c) | AbsConst (f c)

-- * Elements and types

data El :: Case -> Type where
  LocalVar :: BId -> El N
  GlobalVar :: Name -> V.El N -> El N
  Code :: Ty c -> El c
  Lam :: Ty N -> Abs El c -> El c
  App :: El N -> El N -> El N
  Cons :: Dict (El c) -> El c
  Proj :: El N -> Name -> El N
  Lit :: Literal -> El N
  Is :: El N -> El D

data FunctionType ty = FunctionType
  { variant :: FunctionVariant
  , dom :: ty N
  , cod :: Abs ty N
  }

data RecordType ty = RecordType
  { level :: Level
  , fieldTypes :: Dict (ty N)
  }

data EqualityType el ty = EqualityType
  { at :: ty N
  , lhs :: el N
  , rhs :: el N
  }

data Ty :: Case -> Type where
  U :: Universe -> Ty N
  Decode :: El N -> Ty N
  Function :: FunctionType Ty -> Ty N
  Record :: RecordType Ty -> Ty D
  Eq :: EqualityType El Ty -> Ty N
  BuiltinTy :: BuiltinTy -> Ty N

data TypeBehavior
  = LikeU Universe
  | LikeFunction (FunctionType Ty)
  | LikeRecord (RecordType Ty)
  | LikeBuiltinTy BuiltinTy
  | NoRules

-- * Globals

data GlobalEntry = GlobalEntry
  { syn :: El D
  , val :: V.El N
  , ty :: V.Ty N
  }

data Globals = Globals
  { entries :: Map Name GlobalEntry
  , order :: Bwd Name
  }

emptyGlobals :: Globals
emptyGlobals = Globals Map.empty BwdNil

addGlobalEntry :: Name -> GlobalEntry -> Globals -> Globals
addGlobalEntry n e (Globals es o) = Globals (Map.insert n e es) (o :> n)

instance Lookup Globals Name GlobalEntry where
  lookup gs x = Map.lookup x gs.entries

