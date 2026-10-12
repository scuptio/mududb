use mudu::common::buf::Buf;
use std::ops;

/// A binary datum: an owned byte buffer produced or consumed by the binary
/// type implementation.
#[derive(Clone, Debug, Default)]
pub struct DataBinary {
    datum: Buf,
}

impl DataBinary {
    /// Wraps a byte buffer into a binary datum.
    pub fn from(buf: Buf) -> Self {
        Self { datum: buf }
    }

    /// Returns the underlying byte buffer.
    pub fn buf(&self) -> &Buf {
        &self.datum
    }

    /// Consumes the datum and returns the underlying byte buffer.
    pub fn into(self) -> Buf {
        self.datum
    }

    /// Returns the datum bytes as a slice.
    pub fn as_slice(&self) -> &[u8] {
        self.datum.as_slice()
    }
}

impl AsRef<[u8]> for DataBinary {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl ops::Deref for DataBinary {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &[u8] {
        self.as_ref()
    }
}
