use mudu::common::buf::Buf;
use std::ops;

#[derive(Clone)]
pub struct DatBinary {
    datum: Buf,
}

impl DatBinary {
    pub fn from(buf: Buf) -> Self {
        Self { datum: buf }
    }

    pub fn buf(&self) -> &Buf {
        &self.datum
    }

    pub fn into(self) -> Buf {
        self.datum
    }

    pub fn as_slice(&self) -> &[u8] {
        self.datum.as_slice()
    }
}

impl Default for DatBinary {
    fn default() -> Self {
        Self {
            datum: Buf::default(),
        }
    }
}

impl AsRef<[u8]> for DatBinary {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl ops::Deref for DatBinary {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &[u8] {
        self.as_ref()
    }
}
