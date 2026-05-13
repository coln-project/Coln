// ── Reading helpers ─────────────────────────────────────────────────────────

use crate::commit::error::PersistError;

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
