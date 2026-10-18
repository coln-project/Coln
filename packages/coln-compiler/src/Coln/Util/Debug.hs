-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

module Coln.Util.Debug where

#ifdef DEBUG
import GHC.Stack
#endif

-- Panics
--------------------------------------------------------------------------------

#ifdef DEBUG
type Dbg = HasCallStack
#else
type Dbg = () :: Constraint
#endif

panic :: (Dbg) => String -> a
panic invariant = error $ "invariant violated: " ++ invariant

unimplemented :: (Dbg) => a
unimplemented = error "unimplemented"

unwrap :: Maybe a -> a
unwrap (Just x) = x
unwrap Nothing = panic "should only unwrap a Just"
