-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT
{-# LANGUAGE CPP #-}
{-# LANGUAGE DerivingVia #-}
{-# LANGUAGE DuplicateRecordFields #-}
{-# LANGUAGE MagicHash #-}
{-# LANGUAGE PatternSynonyms #-}
{-# LANGUAGE TemplateHaskell #-}
{-# LANGUAGE TypeFamilies #-}
{-# LANGUAGE UnboxedTuples #-}
{-# LANGUAGE UndecidableInstances #-}

-- | Auto-generated direct bindings to the C API of the `coln-store-ffi` Rust crate.
module Coln.Store.Raw where

import Control.Monad.IO.Class (liftIO)
import Data.Function (applyWhen)
import Data.List (isPrefixOf)
import HsBindgen.TH
import System.Environment (getProgName)

do
  progName <- liftIO getProgName
  withHsBindgen
    def
      { clang = def{extraIncludeDirs = [Pkg "include"]}
      , bindingSpec =
          def
            { prescriptiveBindingSpec =
                Just $
                  -- TODO oof this is hideous...
                  -- I'm sure there was some recent Cabal PR about treating package roots more consistently
                  -- but I can't find it right now
                  applyWhen
                    (".haskell-language-server-" `isPrefixOf` progName)
                    ("packages/coln-store-hs/" <>)
                    "binding-spec.yaml"
            }
      , fieldNamingStrategy = OmitFieldPrefixes
      }
    def
      { categoryChoice =
          def
            { cUnsafe = ExcludeCategory
            , cFunPtr = IncludeTermCategory $ RenameTerm (<> "_funptr")
            }
      }
    $ hashInclude "coln_store.h"
