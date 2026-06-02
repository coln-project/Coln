use crate::commit::{error::CodecError, utils::read_slice};

// The write helpers append to an in-memory `Vec<u8>`, whose `io::Write` impl
// never returns an error (allocation failure aborts rather than erroring), so
// they are infallible and return `()`.

pub(crate) fn write_u64(buf: &mut Vec<u8>, value: u64) {
    ::leb128::write::unsigned(buf, value).expect("writing leb128 to a Vec is infallible");
}

pub(crate) fn write_u32(buf: &mut Vec<u8>, value: u32) {
    write_u64(buf, u64::from(value));
}

pub(crate) fn write_len(buf: &mut Vec<u8>, len: usize) {
    write_u64(buf, len as u64);
}

pub(crate) fn write_i64(buf: &mut Vec<u8>, value: i64) {
    ::leb128::write::signed(buf, value).expect("writing leb128 to a Vec is infallible");
}

pub(crate) fn write_len_prefixed_bytes(buf: &mut Vec<u8>, bytes: &[u8]) {
    write_len(buf, bytes.len());
    buf.extend_from_slice(bytes);
}

pub(crate) fn read_u64(data: &[u8], pos: &mut usize, ctx: &'static str) -> Result<u64, CodecError> {
    let mut result = 0u64;
    let mut shift = 0;

    loop {
        let byte = read_byte(data, pos, ctx)?;
        result |= u64::from(byte & 0x7f) << shift;
        shift += 7;

        if byte & 0x80 == 0 {
            if shift > 64 && byte > 1 {
                return Err(CodecError::DataFormatError(format!(
                    "leb128 too large while reading {ctx}"
                )));
            }
            if shift > 7 && byte == 0 {
                return Err(CodecError::DataFormatError(format!(
                    "overlong leb128 while reading {ctx}"
                )));
            }
            return Ok(result);
        }

        if shift > 64 {
            return Err(CodecError::DataFormatError(format!(
                "leb128 too large while reading {ctx}"
            )));
        }
    }
}

pub(crate) fn read_u32(data: &[u8], pos: &mut usize, ctx: &'static str) -> Result<u32, CodecError> {
    let value = read_u64(data, pos, ctx)?;
    value.try_into().map_err(|_| {
        CodecError::DataFormatError(format!("leb128 too large for u32 while reading {ctx}"))
    })
}

pub(crate) fn read_len(
    data: &[u8],
    pos: &mut usize,
    ctx: &'static str,
) -> Result<usize, CodecError> {
    let value = read_u64(data, pos, ctx)?;
    value.try_into().map_err(|_| {
        CodecError::DataFormatError(format!("leb128 too large for usize while reading {ctx}"))
    })
}

pub(crate) fn read_i64(data: &[u8], pos: &mut usize, ctx: &'static str) -> Result<i64, CodecError> {
    let mut result = 0i64;
    let mut shift = 0;
    let mut previous = 0;

    loop {
        let byte = read_byte(data, pos, ctx)?;
        result |= i64::from(byte & 0x7f) << shift;
        shift += 7;

        if byte & 0x80 == 0 {
            if shift > 64 && byte != 0 && byte != 0x7f {
                return Err(CodecError::DataFormatError(format!(
                    "leb128 too large while reading {ctx}"
                )));
            }
            if shift > 7
                && ((byte == 0 && previous & 0x40 == 0) || (byte == 0x7f && previous & 0x40 > 0))
            {
                return Err(CodecError::DataFormatError(format!(
                    "overlong leb128 while reading {ctx}"
                )));
            }
            if shift < 64 && byte & 0x40 > 0 {
                result |= -1i64 << shift;
            }
            return Ok(result);
        }

        if shift > 64 {
            return Err(CodecError::DataFormatError(format!(
                "leb128 too large while reading {ctx}"
            )));
        }
        previous = byte;
    }
}

pub(crate) fn read_len_prefixed_bytes<'a>(
    data: &'a [u8],
    pos: &mut usize,
    ctx: &'static str,
) -> Result<&'a [u8], CodecError> {
    let len = read_len(data, pos, ctx)?;
    read_slice(data, pos, len, ctx)
}

fn read_byte(data: &[u8], pos: &mut usize, ctx: &'static str) -> Result<u8, CodecError> {
    Ok(read_slice(data, pos, 1, ctx)?[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u64_round_trips() {
        for value in [0, 1, 127, 128, 16_383, u64::MAX] {
            let mut encoded = Vec::new();
            write_u64(&mut encoded, value);

            let mut pos = 0;
            let decoded = read_u64(&encoded, &mut pos, "value").expect("read uleb");

            assert_eq!(decoded, value);
            assert_eq!(pos, encoded.len());
        }
    }

    #[test]
    fn i64_round_trips() {
        for value in [i64::MIN, -129, -128, -1, 0, 1, 127, 128, i64::MAX] {
            let mut encoded = Vec::new();
            write_i64(&mut encoded, value);

            let mut pos = 0;
            let decoded = read_i64(&encoded, &mut pos, "value").expect("read sleb");

            assert_eq!(decoded, value);
            assert_eq!(pos, encoded.len());
        }
    }

    #[test]
    fn reads_external_unsigned_leb128_encodings() {
        for value in [0, 1, 127, 128, 16_383, u64::MAX] {
            let mut encoded = Vec::new();
            ::leb128::write::unsigned(&mut encoded, value).expect("external write uleb");

            let mut pos = 0;
            let decoded = read_u64(&encoded, &mut pos, "value").expect("read uleb");

            assert_eq!(decoded, value);
            assert_eq!(pos, encoded.len());
        }
    }

    #[test]
    fn reads_external_signed_leb128_encodings() {
        for value in [i64::MIN, -129, -128, -1, 0, 1, 127, 128, i64::MAX] {
            let mut encoded = Vec::new();
            ::leb128::write::signed(&mut encoded, value).expect("external write sleb");

            let mut pos = 0;
            let decoded = read_i64(&encoded, &mut pos, "value").expect("read sleb");

            assert_eq!(decoded, value);
            assert_eq!(pos, encoded.len());
        }
    }

    #[test]
    fn u64_rejects_overlong_encoding() {
        let permissive = ::leb128::read::unsigned(&mut &[0x81, 0x00][..])
            .expect("leb128 crate accepts overlong encodings");
        assert_eq!(permissive, 1);

        let mut pos = 0;
        let err = read_u64(&[0x81, 0x00], &mut pos, "value").expect_err("overlong");

        assert!(matches!(err, CodecError::DataFormatError(_)));
    }

    #[test]
    fn u32_rejects_too_large_value() {
        let mut encoded = Vec::new();
        write_u64(&mut encoded, u64::from(u32::MAX) + 1);

        let mut pos = 0;
        let err = read_u32(&encoded, &mut pos, "value").expect_err("too large");

        assert!(matches!(err, CodecError::DataFormatError(_)));
    }

    #[test]
    fn length_prefixed_bytes_round_trip() {
        let mut encoded = Vec::new();
        write_len_prefixed_bytes(&mut encoded, b"abc");

        let mut pos = 0;
        let decoded = read_len_prefixed_bytes(&encoded, &mut pos, "bytes").expect("read bytes");

        assert_eq!(decoded, b"abc");
        assert_eq!(pos, encoded.len());
    }
}
