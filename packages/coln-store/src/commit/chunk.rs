use crate::commit::{
    Commit, CommitHash,
    error::CodecError,
    leb128 as commit_leb128,
    utils::{read_slice, read_u8},
};

/// Chunk magic bytes
const MAGIC: &[u8; 4] = b"GMcm";

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ChunkType {
    Commit = 0,
    Root = 1,
}

impl From<ChunkType> for u8 {
    fn from(ct: ChunkType) -> u8 {
        match ct {
            ChunkType::Commit => 0,
            ChunkType::Root => 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chunk {
    Commit { header: Header, payload: Vec<u8> },
    Root { header: Header, payload: Vec<u8> },
}

impl Chunk {
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

    pub(crate) fn chunk_type(&self) -> ChunkType {
        match self {
            Chunk::Commit { header, .. } | Chunk::Root { header, .. } => header.chunk_type,
        }
    }

    /// Writes this chunk as `header || payload` into `out`.
    /// Convenience function for store serialization
    pub(crate) fn write(&self, out: &mut Vec<u8>) {
        match self {
            Chunk::Commit { header, payload } | Chunk::Root { header, payload } => {
                header.write(out);
                out.extend_from_slice(payload);
            }
        }
    }

    // Intended for network sync, where the transport wants one framed chunk.
    pub fn encoded(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.write(&mut buf);
        buf
    }

    pub(crate) fn into_parts(self) -> (Header, Vec<u8>) {
        match self {
            Chunk::Commit { header, payload } | Chunk::Root { header, payload } => {
                (header, payload)
            }
        }
    }

    fn from_parts(header: Header, payload: Vec<u8>) -> Self {
        debug_assert_eq!(header.data_len, payload.len());
        match header.chunk_type {
            ChunkType::Commit => Chunk::Commit { header, payload },
            ChunkType::Root => Chunk::Root { header, payload },
        }
    }
}

impl From<&Commit<'_>> for Chunk {
    fn from(commit: &Commit<'_>) -> Self {
        Self::from_parts(commit.header.clone(), commit.payload().to_vec())
    }
}

impl From<Commit<'_>> for Chunk {
    fn from(commit: Commit<'_>) -> Self {
        let Commit { header, bytes, .. } = commit;
        Self::from_parts(header, bytes.into_owned())
    }
}

/// Compute the content hash for a chunk.
///
/// hash = blake3(chunk_type:u8 || data_len:u64_le || data)
///
/// This mirrors the preimage automerge builds in storage/chunk.rs, adapted for
/// Coln Store chunk types, but hashes it with BLAKE3 rather than SHA-256.
pub(crate) fn hash(chunk_type: ChunkType, data: &[u8]) -> CommitHash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[u8::from(chunk_type)]);
    hasher.update(&(data.len() as u64).to_le_bytes());
    hasher.update(data);
    CommitHash(hasher.finalize().into())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CheckSum([u8; 4]);

impl CheckSum {
    pub(crate) fn bytes(&self) -> [u8; 4] {
        self.0
    }
}

impl From<[u8; 4]> for CheckSum {
    fn from(raw: [u8; 4]) -> Self {
        CheckSum(raw)
    }
}

impl AsRef<[u8]> for CheckSum {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<CommitHash> for CheckSum {
    fn from(h: CommitHash) -> Self {
        let bytes = h.as_bytes();
        [bytes[0], bytes[1], bytes[2], bytes[3]].into()
    }
}

/// Chunk framing that precedes the canonical payload on disk and on the wire:
/// `[MAGIC][checksum:4][chunk_type:1][data_len:uleb]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub(crate) chunk_type: ChunkType,
    pub(crate) data_len: usize,
    checksum: CheckSum,          // first four bytes of the hash
    pub(crate) hash: CommitHash, // not serialized
}

impl Header {
    /// Build the framing for `data`, deriving the hash and checksum from it.
    pub(crate) fn new(chunk_type: ChunkType, data: &[u8]) -> Self {
        let hash = hash(chunk_type, data);
        Self {
            chunk_type,
            hash,
            data_len: data.len(),
            checksum: hash.checksum().into(),
        }
    }

    /// Write the framing bytes that precede the payload.
    pub(crate) fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&self.checksum.bytes());
        out.push(u8::from(self.chunk_type));
        commit_leb128::write_u64(out, self.data_len as u64);
    }

    /// Parse a chunk header at `pos`, leaving `pos` at the start of the payload.
    ///
    /// Recomputes the payload hash and rejects the chunk when the stored checksum
    /// does not match, so a corrupted payload is caught before it is decoded.
    pub(crate) fn parse(data: &[u8], pos: &mut usize) -> Result<Self, CodecError> {
        let magic = read_slice(data, pos, MAGIC.len(), "chunk magic")?;
        if magic != MAGIC {
            return Err(CodecError::DataFormatError("bad chunk magic".into()));
        }

        let checksum_bytes = read_slice(data, pos, 4, "chunk checksum")?;
        let checksum: CheckSum = <[u8; 4]>::try_from(checksum_bytes)
            .expect("read_slice returned four bytes")
            .into();

        let chunk_type = match read_u8(data, pos, "chunk type")? {
            0 => ChunkType::Commit,
            1 => ChunkType::Root,
            tag => {
                return Err(CodecError::DataFormatError(format!(
                    "unknown chunk type {tag}"
                )));
            }
        };

        let data_len = commit_leb128::read_len(data, pos, "chunk data_len")?;
        let end = pos
            .checked_add(data_len)
            .ok_or_else(|| CodecError::DataFormatError("chunk length overflow".into()))?;
        let body = data
            .get(*pos..end)
            .ok_or_else(|| CodecError::DataFormatError("truncated chunk payload".into()))?;

        let header = Self {
            chunk_type,
            hash: hash(chunk_type, body),
            data_len,
            checksum,
        };
        if !header.checksum_valid() {
            return Err(CodecError::ChecksumMismatch);
        }
        Ok(header)
    }

    fn checksum_valid(&self) -> bool {
        CheckSum(self.hash.checksum()) == self.checksum
    }
}
