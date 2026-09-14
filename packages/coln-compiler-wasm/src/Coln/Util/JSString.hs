{-# LANGUAGE MagicHash #-}
{-# LANGUAGE UnliftedFFITypes #-}

module Coln.Util.JSString
  ( textToJSString
  , byteStringToJSString
  , textFromJSString
  ) where

import Control.Monad.ST (stToIO)
import Data.ByteString (ByteString)
import Data.ByteString qualified as BS
import Data.Array.Byte
  ( ByteArray(..)
  , MutableByteArray(..)
  )
import Data.Text (Text)
import qualified Data.Text.Array as A
import qualified Data.Text.Internal as T
import Foreign.Ptr
import Foreign.C.Types
import GHC.Exts
  ( ByteArray#
  , MutableByteArray#
  , RealWorld
  )
import GHC.Wasm.Prim
import System.IO.Unsafe
  ( unsafeDupablePerformIO
  , unsafePerformIO
  )


-- Keep one encoder/decoder around rather than constructing one
-- for every conversion.

foreign import javascript unsafe
  "new TextDecoder('utf-8', {fatal: true})"
  js_newTextDecoder :: IO JSVal

foreign import javascript unsafe
  "new TextEncoder()"
  js_newTextEncoder :: IO JSVal

{-# NOINLINE textDecoder #-}
textDecoder :: JSVal
textDecoder = unsafePerformIO js_newTextDecoder

{-# NOINLINE textEncoder #-}
textEncoder :: JSVal
textEncoder = unsafePerformIO js_newTextEncoder


-- Text -> JSString
--
-- $2 is the address of the ByteArray# payload in linear memory.

foreign import javascript unsafe
  "$1.decode(new Uint8Array(__exports.memory.buffer, $2 + $3, $4))"
  js_decodeUtf8
    :: JSVal
    -> ByteArray#
    -> Int             -- offset
    -> Int             -- length
    -> IO JSString

textToJSStringIO :: Text -> IO JSString
textToJSStringIO (T.Text (ByteArray ba#) off len) =
  js_decodeUtf8 textDecoder ba# off len

textToJSString :: Text -> JSString
textToJSString =
  unsafeDupablePerformIO . textToJSStringIO

foreign import javascript unsafe
  "$1.decode(new Uint8Array(__exports.memory.buffer, $2, $3))"
   js_decodeUtf8Ptr
     :: JSVal
     -> Ptr CChar
     -> Int
     -> IO JSString

byteStringToJSString :: ByteString -> JSString
byteStringToJSString bs = unsafeDupablePerformIO $
  BS.useAsCStringLen bs $ \(p, n) ->
    js_decodeUtf8Ptr textDecoder p n

-- JSString -> Text

foreign import javascript unsafe
  "$1.length"
  js_stringLength :: JSString -> IO Int

-- $3 is the address of the MutableByteArray# payload.
foreign import javascript unsafe
  "$1.encodeInto($2, new Uint8Array(__exports.memory.buffer, $3, $4)).written"
  js_encodeUtf8
    :: JSVal
    -> JSString
    -> MutableByteArray# RealWorld
    -> Int             -- destination capacity
    -> IO Int          -- bytes written

textFromJSStringIO :: JSString -> IO Text
textFromJSStringIO s = do
  -- JavaScript .length is the number of UTF-16 code units.
  --
  -- Three UTF-8 bytes per UTF-16 code unit is always sufficient:
  --   BMP code point:        1 code unit  -> <= 3 bytes
  --   surrogate pair:        2 code units -> 4 bytes
  --   unpaired surrogate:    1 code unit  -> U+FFFD -> 3 bytes
  n <- js_stringLength s
  let capacity = 3 * n

  marr@(MutableByteArray mba#) <- stToIO (A.new capacity)

  written <-
    js_encodeUtf8 textEncoder s mba# capacity

  arr <- stToIO $ do
    -- Otherwise an ASCII JS string would leave us retaining an array
    -- three times larger than necessary.
    A.shrinkM marr written
    A.unsafeFreeze marr

  pure $ T.text arr 0 written

textFromJSString :: JSString -> Text
textFromJSString =
  unsafeDupablePerformIO . textFromJSStringIO
