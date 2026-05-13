use std::fmt;

/// The number of bytes in a commit hash.
pub(crate) const HASH_SIZE: usize = 32;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct CommitHash(pub [u8; HASH_SIZE]);

impl CommitHash {
    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8]> for CommitHash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Display for CommitHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}
