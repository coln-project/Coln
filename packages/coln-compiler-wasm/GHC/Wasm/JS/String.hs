-- copied verbatim from: https://github.com/haskell-wasm/canary/blob/e4918e5f69750aab75e878c8315b650d8578e0c8/src/GHC/Wasm/JS/String.hs

{-# LANGUAGE MagicHash #-}

module GHC.Wasm.JS.String
  ( JSString (..),
    fromStrictByteString,
    toStrictByteString,
    fromShortByteString,
    toShortByteString,
    fromStrictText,
    toStrictText,
  )
where

import Data.ByteString.Internal qualified as BS
import Data.ByteString.Short qualified as SBS
import Data.ByteString.Unsafe qualified as BS
import Data.Primitive
import Data.Text.Internal qualified as T
import GHC.Exts
import GHC.ForeignPtr
import GHC.Wasm.Prim
import System.IO.Unsafe

fromStrictByteString :: BS.ByteString -> JSString
fromStrictByteString bs =
  unsafePerformIO $ BS.unsafeUseAsCStringLen bs $ uncurry utf8DecodePtr

toStrictByteString :: JSString -> BS.ByteString
toStrictByteString str = case utf8EncodeBufferEstimateSize str of
  0 -> BS.empty
  len' -> unsafePerformIO $ do
    mba@(MutableByteArray mba#) <- newPinnedByteArray len'
    len <- utf8EncodeIntoMutableByteArray# str mba# len'
    shrinkMutableByteArray mba len
    pure $
      BS.fromForeignPtr0
        (ForeignPtr (mutableByteArrayContents# mba#) (PlainPtr mba#))
        len

fromShortByteString :: SBS.ShortByteString -> JSString
fromShortByteString (SBS.ShortByteString ba) =
  fromStrictText $ T.Text ba 0 $ sizeofByteArray ba

toShortByteString :: JSString -> SBS.ShortByteString
toShortByteString str = SBS.ShortByteString ba
  where
    T.Text ba _ _ = toStrictText str

fromStrictText :: T.Text -> JSString
fromStrictText (T.Text (ByteArray ba#) off len) =
  utf8DecodeByteArray# ba# off len

toStrictText :: JSString -> T.Text
toStrictText str = case utf8EncodeBufferEstimateSize str of
  0 -> T.empty
  len' -> unsafePerformIO $ do
    mba@(MutableByteArray mba#) <- newByteArray len'
    len <- utf8EncodeIntoMutableByteArray# str mba# len'
    shrinkMutableByteArray mba len
    ba <- unsafeFreezeByteArray mba
    pure $ T.Text ba 0 len

foreign import javascript unsafe "(new TextDecoder('utf-8', {fatal: true})).decode(new Uint8Array(__exports.memory.buffer, $1, $2))"
  utf8DecodePtr ::
    Ptr a -> Int -> IO JSString

foreign import javascript unsafe "$1.length * 3"
  utf8EncodeBufferEstimateSize ::
    JSString -> Int

foreign import javascript unsafe "(new TextDecoder('utf-8', {fatal: true})).decode(new Uint8Array(__exports.memory.buffer, $1 + $2, $3))"
  utf8DecodeByteArray# ::
    ByteArray# -> Int -> Int -> JSString

foreign import javascript unsafe "(new TextEncoder()).encodeInto($1, new Uint8Array(__exports.memory.buffer, $2, $3)).written"
  utf8EncodeIntoMutableByteArray# ::
    JSString -> MutableByteArray# s -> Int -> IO Int
