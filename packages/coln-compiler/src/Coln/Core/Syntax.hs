-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Core.Syntax where

import Coln.Common
import Coln.Core.Params
import Coln.Core.Value qualified as V

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
  Lookup :: TableName -> Dict (El N) -> Ty N -> El N

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
  IsTy :: Ty N -> Ty D
  EltOf :: TableName -> Dict (El N) -> Ty N

data TypeBehavior
  = LikeU Universe
  | LikeFunction (FunctionType Ty)
  | LikeRecord (RecordType Ty)
  | LikeBuiltinTy BuiltinTy
  | NoRules
