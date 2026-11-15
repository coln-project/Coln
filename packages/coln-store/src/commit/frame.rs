// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::commit::{
    Commit,
    chunk::Chunk,
    error::CodecError,
    header::{ChunkType, Header},
    utils::read_slice,
};

// A `Frame` is just raw data separated into header and payload, during the first
// stage of parsing bytes
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Frame {
    header: Header,
    payload: Vec<u8>,
}

impl Frame {
    pub(crate) fn read_at(data: &[u8], pos: &mut usize) -> Result<Self, CodecError> {
        let header = Header::parse(data, pos)?;
        let payload = read_slice(data, pos, header.data_len, "chunk payload")?.to_vec();
        Ok(Self::from_parts(header, payload))
    }

    // Intended for network sync, where the transport hands us one framed chunk.
    pub fn decode(data: &[u8]) -> Result<Self, CodecError> {
        let mut pos = 0;
        let chunk = Self::read_at(data, &mut pos)?;
        if pos != data.len() {
            return Err(CodecError::DataFormatError(format!(
                "trailing bytes after chunk: {} bytes",
                data.len() - pos
            )));
        }
        Ok(chunk)
    }

    pub fn is_root(&self) -> bool {
        self.chunk_type() == ChunkType::RootCommit
    }

    pub(crate) fn chunk_type(&self) -> ChunkType {
        self.header.chunk_type
    }

    pub(crate) fn into_parts(self) -> (Header, Vec<u8>) {
        (self.header, self.payload)
    }

    fn from_parts(header: Header, payload: Vec<u8>) -> Self {
        debug_assert_eq!(header.data_len, payload.len());
        Self { header, payload }
    }

    /// Writes this chunk as `header || payload` into `out`.
    /// Convenience function for store serialization
    pub(crate) fn write(&self, out: &mut Vec<u8>) {
        self.header.write(out);
        out.extend_from_slice(&self.payload);
    }

    // Intended for network sync, where the transport wants one framed chunk.
    pub fn encoded(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.write(&mut buf);
        buf
    }
}

impl<T> From<&Commit<'_, T>> for Frame {
    fn from(commit: &Commit<'_, T>) -> Self {
        Self::from_parts(commit.header.clone(), commit.payload().to_vec())
    }
}

impl<T> From<Commit<'_, T>> for Frame {
    fn from(commit: Commit<'_, T>) -> Self {
        let Commit { header, bytes, .. } = commit;
        Self::from_parts(header, bytes.into_owned())
    }
}

impl From<&Chunk> for Frame {
    fn from(chunk: &Chunk) -> Self {
        match chunk {
            Chunk::Root(commit) => Self::from(commit),
            Chunk::Data(commit) => Self::from(commit),
        }
    }
}

impl From<Chunk> for Frame {
    fn from(chunk: Chunk) -> Self {
        match chunk {
            Chunk::Root(commit) => Self::from(commit),
            Chunk::Data(commit) => Self::from(commit),
        }
    }
}
