-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT
{-# LANGUAGE UndecidableInstances #-}

module Coln.Core.Syntax where

import Coln.Common
import Coln.Core.Params
import Coln.Core.Value qualified as V

-- Abstractions
--------------------------------------------------------------------------------

data Abs (f :: Case -> Type) (c :: Case) = Abs Name (f c) | AbsConst (f c)

deriving instance (Show (f c)) => Show (Abs f c)

data MemoedGlobal = MemoedGlobal
  { name :: Name
  , value :: V.El N
  }

instance Show MemoedGlobal where
  show mg = show mg.name

-- * Elements and types

data El :: Case -> Type where
  LocalVar :: BId -> El N
  GlobalVar :: MemoedGlobal -> El N
  Code :: Universe -> Ty c -> El c
  Lam :: FunctionVariant -> Ty N -> Abs El c -> El c
  App :: FunctionVariant -> El N -> El N -> El N
  Cons :: Level -> Dict (El c) -> El c
  Proj :: Level -> El N -> Name -> El N
  Init :: Ty N -> El D
  Lit :: Literal -> El N
  Is :: El N -> El D

deriving instance Show (El c)

data FunctionType ty = FunctionType
  { variant :: FunctionVariant
  , dom :: ty N
  , cod :: Abs ty N
  }

deriving instance (Show (ty N)) => Show (FunctionType ty)

data RecordType ty = RecordType
  { level :: Level
  , fieldTypes :: Dict (ty N)
  }

deriving instance (Show (ty N)) => Show (RecordType ty)

data EqualityType el ty = EqualityType
  { at :: ty N
  , lhs :: el N
  , rhs :: el N
  }

deriving instance (Show (ty N), Show (el N)) => Show (EqualityType el ty)

data Ty :: Case -> Type where
  U :: Universe -> Ty N
  Decode :: Universe -> El N -> Ty N
  Function :: FunctionType Ty -> Ty N
  Record :: RecordType Ty -> Ty D
  Eq :: EqualityType El Ty -> Ty N
  BuiltinTy :: BuiltinTy -> Ty N
  IsTy :: Ty N -> Ty D

deriving instance Show (Ty c)

data TypeBehavior
  = LikeU Universe
  | LikeFunction (FunctionType Ty)
  | LikeRecord (RecordType Ty)
  | LikeInductive (El N)
  | LikeBuiltinTy BuiltinTy
  | NoRules
