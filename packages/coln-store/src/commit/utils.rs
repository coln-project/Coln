// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::io::Write;

use crate::commit::error::CodecError;

// ── Write helpers ───────────────────────────────────────────────────────────

pub(crate) fn write_u8(buf: &mut Vec<u8>, value: u8) -> Result<(), CodecError> {
    buf.write_all(&[value])?;
    Ok(())
}

// ── Read helpers ────────────────────────────────────────────────────────────

pub(crate) fn read_u8(data: &[u8], pos: &mut usize, ctx: &'static str) -> Result<u8, CodecError> {
    Ok(read_slice(data, pos, 1, ctx)?[0])
}

pub(crate) fn read_slice<'a>(
    data: &'a [u8],
    pos: &mut usize,
    len: usize,
    ctx: &'static str,
) -> Result<&'a [u8], CodecError> {
    if data.len() < *pos + len {
        return Err(CodecError::DataFormatError(format!(
            "truncated while reading {ctx}"
        )));
    }
    let slice = &data[*pos..*pos + len];
    *pos += len;
    Ok(slice)
}
