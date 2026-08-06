-- SPDX-FileCopyrightText: 2026 Coln contributors
--
-- SPDX-License-Identifier: Apache-2.0 OR MIT

{- | An idiomatic Haskell API around the `coln-store-ffi` Rust crate.
These are boilerplate-y wrappers around the raw bindings in `Coln.Store.Raw`.
Eventually, `hs-bindgen` should automate more of this for us.
We should keep an eye on upstream progress: https://github.com/well-typed/hs-bindgen/issues?q=label%3Ahighlevel
(particularly https://github.com/well-typed/hs-bindgen/issues/8#issuecomment-4535882902).
In the meantime, LLMs can be quite good at generating this sort of code,
at least now that we've established a good style for them to conform to.
-}
module Coln.Store.Wrapped (
  Theory,
  theoryFromJson,
  theoryToJson,
  theoryDebug,
  Store,
  storeFromTheory,
  storeDump,
)
where

import Coln.Store.Raw qualified as Raw
import Data.ByteString (ByteString)
import Data.ByteString qualified as BS
import Data.Text (Text)
import Data.Text.Encoding (encodeUtf8)
import Foreign
import Foreign.C
import Foreign.C.ConstPtr
import System.IO.Unsafe

newtype Theory = Theory (ForeignPtr Raw.FlatRealm)

theoryFromJson :: Text -> IO (Either ByteString Theory)
theoryFromJson json =
  BS.useAsCString (encodeUtf8 json) \cstr -> do
    res <- Raw.theory_from_json $ ConstPtr cstr
    case res.tag of
      Raw.ErrFlatRealmCChar -> Left <$> copyCString res.err
      Raw.OkFlatRealmCChar -> Right . Theory <$> newForeignPtr Raw.free_theory_funptr (unConstPtr res.ok)

{-# NOINLINE theoryToJson #-}
theoryToJson :: Theory -> ByteString
theoryToJson (Theory p) = unsafePerformIO $ copyCString =<< withForeignPtr p (Raw.theory_to_json . ConstPtr)

{-# NOINLINE theoryDebug #-}
theoryDebug :: Theory -> ByteString
theoryDebug (Theory p) = unsafePerformIO $ copyCString =<< withForeignPtr p (Raw.theory_debug . ConstPtr)

newtype Store = Store (ForeignPtr Raw.Store)

storeFromTheory :: Theory -> IO (Either ByteString Store)
storeFromTheory (Theory fptr) =
  withForeignPtr fptr \ptr -> do
    res <- Raw.store_from_theory $ ConstPtr ptr
    case res.tag of
      Raw.ErrStoreCChar -> Left <$> copyCString res.err
      Raw.OkStoreCChar -> Right . Store <$> newForeignPtr Raw.free_store_funptr (unConstPtr res.ok)

{-# NOINLINE storeDump #-}
storeDump :: Store -> ByteString
storeDump (Store p) = unsafePerformIO $ copyCString =<< withForeignPtr p (Raw.store_dump . ConstPtr)

copyCString :: ConstPtr CChar -> IO ByteString
copyCString cstr = do
  bs <- BS.packCString $ unConstPtr cstr
  Raw.free_string $ unConstPtr cstr
  pure bs
