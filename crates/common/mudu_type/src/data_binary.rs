use mudu::common::buf::Buf;
use std::ops;

#[derive(Clone, Debug, Default)]
pub struct DataBinary {
    datum: Buf,
}

impl DataBinary {
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
