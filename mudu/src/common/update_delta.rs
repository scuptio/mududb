use crate::common::bc_dec::{DecErr, Decode, Decoder};
use crate::common::bc_enc::{EncErr, Encode, Encoder};
use crate::common::buf::Buf;
#[cfg(any(test, feature = "test"))]
use arbitrary::{Arbitrary, Unstructured};
use std::cell::RefCell;

const TUPLE_MAX_LEN_DEFAULT: usize = 100;

thread_local! {
    static  TUPLE_MAX_LEN:RefCell<usize> = RefCell::new(TUPLE_MAX_LEN_DEFAULT);
}

#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct UpdateDelta {
    offset: u32,
    size: u32,
    data: Buf,
}

impl UpdateDelta {
    pub fn arb_set_tuple_max_len(len: usize) {
        assert!(len > 0);
        TUPLE_MAX_LEN.replace(len);
    }

    pub fn tuple_max_len() -> usize {
        let mut n = TUPLE_MAX_LEN.take();
        if n == 0 {
            TUPLE_MAX_LEN.replace(TUPLE_MAX_LEN_DEFAULT);
            n = TUPLE_MAX_LEN_DEFAULT;
        }
        n
    }

    pub fn new(offset: u32, size: u32, data: Buf) -> Self {
        Self { offset, size, data }
    }

    pub fn to_up_size(&self) -> u32 {
        self.size
    }

    pub fn offset(&self) -> u32 {
        self.offset
    }

    pub fn delta(&self) -> &Buf {
        &self.data
    }

    // apply and return the compensation operation
    pub fn apply_to(&self, buf: &mut Buf) -> Self {
        if buf.len() < (self.offset + self.size) as usize {
            panic!(
                "size error, offset:{}, size: {} but tuple length {}",
                self.offset,
                self.size,
                buf.len()
            );
        }
        let range = self.offset as usize..(self.offset + self.size) as usize;
        let undo_data = buf.splice(range, self.data.clone()).collect();
        Self {
            offset: self.offset,
            size: self.data.len() as u32,
            data: undo_data,
        }
    }

    pub fn to_replace_size(&self) -> usize {
        self.size as usize
    }

    pub fn to_replace_start(&self) -> usize {
        self.offset as usize
    }

    pub fn to_replace_end(&self) -> usize {
        (self.offset + self.size) as usize
    }
}

impl Encode for UpdateDelta {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncErr> {
        encoder.write_u32(self.offset)?;
        encoder.write_u32(self.size)?;
        encoder.write_u32(self.data.len() as u32)?;
        encoder.write_bytes(self.data.as_slice())?;
        Ok(())
    }

    fn size(&self) -> Result<usize, EncErr> {
        let mut size = 0usize;
        size += size_of_val(&self.offset);
        size += size_of_val(&self.size);
        size += size_of::<u32>();
        size += self.data.len();
        Ok(size)
    }
}

impl Decode for UpdateDelta {
    fn decode<E: Decoder>(decoder: &mut E) -> Result<Self, DecErr> {
        let offset = decoder.read_u32()?;
        let size = decoder.read_u32()?;
        let len = decoder.read_u32()? as usize;
        let mut data = Buf::new();
        data.resize(len, 0);
        decoder.read_bytes(data.as_mut_slice())?;
        Ok(Self { offset, size, data })
    }
}

#[cfg(any(test, feature = "test"))]
impl Arbitrary<'_> for UpdateDelta {
    #[cfg(any(test, feature = "test"))]
    fn arbitrary(u: &mut Unstructured) -> arbitrary::Result<Self> {
        let len = Self::tuple_max_len() as u32;
        let begin = u32::arbitrary(u)? % len;
        let end = u32::arbitrary(u)? % len;
        let (offset, size) = if begin <= end {
            (begin, end - begin)
        } else {
            (begin, 0)
        };
        let data_len = u32::arbitrary(u)? % len;
        let b = u.bytes(data_len as usize)?;
        let uu = Unstructured::new(&b);
        let data = Buf::arbitrary_take_rest(uu)?;
        Ok(Self { offset, size, data })
    }
}
