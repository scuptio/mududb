use crate::common::buf::Buf;

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
}

impl Default for DatBinary {
    fn default() -> Self {
        Self { datum: Buf::default() }
    }
}