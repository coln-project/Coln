use std::fmt;

#[derive(PartialEq, Hash, Clone, Ord, Eq, PartialOrd)]
pub struct Author(Vec<u8>);

impl Author {
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }
    pub fn to_hex_string(&self) -> String {
        hex::encode(&self.0)
    }

    /// Convenience method for creating a place holder author, the author Foo!
    /// Place holder only
    pub(crate) fn foo() -> Self {
        Self::from(vec![0u8; 32])
    }
}

impl fmt::Display for Author {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex_string())
    }
}

impl From<Vec<u8>> for Author {
    fn from(v: Vec<u8>) -> Self {
        Author(v)
    }
}

impl<'a> From<&'a [u8]> for Author {
    fn from(s: &'a [u8]) -> Self {
        Author(s.to_vec())
    }
}

impl AsRef<[u8]> for Author {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for Author {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Author")
            .field(&hex::encode(&self.0))
            .finish()
    }
}
