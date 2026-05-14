use crate::commit::error::PersistError;
use std::io::Write;

// ── Write helpers ───────────────────────────────────────────────────────────

pub(crate) fn write_u8(buf: &mut Vec<u8>, value: u8) -> Result<(), PersistError> {
    buf.write_all(&[value])?;
    Ok(())
}

pub(crate) fn write_i64(buf: &mut Vec<u8>, value: i64) -> Result<(), PersistError> {
    buf.write_all(&value.to_le_bytes())?;
    Ok(())
}

pub(crate) fn write_len_prefixed_bytes(
    buf: &mut Vec<u8>,
    bytes: &[u8],
    ctx: &'static str,
) -> Result<(), PersistError> {
    let len: u64 = bytes
        .len()
        .try_into()
        .map_err(|_| PersistError::Other(ctx.into()))?;
    buf.write_all(&len.to_le_bytes())?;
    buf.write_all(bytes)?;
    Ok(())
}

// ── Read helpers ────────────────────────────────────────────────────────────

pub(crate) fn read_u8(data: &[u8], pos: &mut usize, ctx: &'static str) -> Result<u8, PersistError> {
    Ok(read_slice(data, pos, 1, ctx)?[0])
}

pub(crate) fn read_u32(
    data: &[u8],
    pos: &mut usize,
    ctx: &'static str,
) -> Result<u32, PersistError> {
    if data.len() < *pos + 4 {
        return Err(PersistError::DataFormatError(format!(
            "truncated while reading {ctx}"
        )));
    }
    let v = u32::from_le_bytes(data[*pos..*pos + 4].try_into().unwrap());
    *pos += 4;
    Ok(v)
}

pub(crate) fn read_u64(
    data: &[u8],
    pos: &mut usize,
    ctx: &'static str,
) -> Result<u64, PersistError> {
    if data.len() < *pos + 8 {
        return Err(PersistError::DataFormatError(format!(
            "truncated while reading {ctx}"
        )));
    }
    let v = u64::from_le_bytes(data[*pos..*pos + 8].try_into().unwrap());
    *pos += 8;
    Ok(v)
}

pub(crate) fn read_i64(
    data: &[u8],
    pos: &mut usize,
    ctx: &'static str,
) -> Result<i64, PersistError> {
    let bytes = read_slice(data, pos, 8, ctx)?;
    Ok(i64::from_le_bytes(bytes.try_into().unwrap()))
}

pub(crate) fn read_slice<'a>(
    data: &'a [u8],
    pos: &mut usize,
    len: usize,
    ctx: &'static str,
) -> Result<&'a [u8], PersistError> {
    if data.len() < *pos + len {
        return Err(PersistError::DataFormatError(format!(
            "truncated while reading {ctx}"
        )));
    }
    let slice = &data[*pos..*pos + len];
    *pos += len;
    Ok(slice)
}

pub(crate) fn read_len_prefixed_bytes<'a>(
    data: &'a [u8],
    pos: &mut usize,
    ctx: &'static str,
) -> Result<&'a [u8], PersistError> {
    let len = read_u64(data, pos, ctx)? as usize;
    read_slice(data, pos, len, ctx)
}
