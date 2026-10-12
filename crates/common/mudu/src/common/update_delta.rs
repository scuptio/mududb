//! In-place tuple edit records.
//!
//! An [`UpdateDelta`] describes a single splice into a tuple buffer: it
//! replaces a range of bytes with new data, and applying it produces the
//! compensating delta that undoes the change.

use crate::common::buf::Buf;
use crate::common::codec::{DecErr, Decode, Decoder, EncErr, Encode, Encoder};
#[cfg(any(test, feature = "arbitrary"))]
use arbitrary::{Arbitrary, Unstructured};
use std::cell::RefCell;

const TUPLE_MAX_LEN_DEFAULT: usize = 100;

thread_local! {
    static  TUPLE_MAX_LEN:RefCell<usize> = const { RefCell::new(TUPLE_MAX_LEN_DEFAULT) };
}

/// An in-place edit to a tuple buffer: replaces `size` bytes at `offset` with `data`.
#[derive(Debug, Eq, PartialEq, Clone, Default)]
pub struct UpdateDelta {
    offset: u32,
    size: u32,
    data: Buf,
}

impl UpdateDelta {
    /// Sets the maximum tuple length used when generating arbitrary values.
    pub fn arb_set_tuple_max_len(len: usize) {
        assert!(len > 0);
        TUPLE_MAX_LEN.replace(len);
    }

    /// Resets the tuple length limit so the next access restores the default.
    #[cfg(test)]
    pub fn test_reset_tuple_max_len_to_zero() {
        TUPLE_MAX_LEN.with(|cell| {
            cell.replace(0);
        });
    }

    /// Returns the current maximum tuple length used when generating
    /// arbitrary values, restoring the default after a reset.
    pub fn tuple_max_len() -> usize {
        let mut n = TUPLE_MAX_LEN.take();
        if n == 0 {
            TUPLE_MAX_LEN.replace(TUPLE_MAX_LEN_DEFAULT);
            n = TUPLE_MAX_LEN_DEFAULT;
        }
        n
    }

    /// Creates a delta that replaces `size` bytes at `offset` with `data`.
    pub fn new(offset: u32, size: u32, data: Buf) -> Self {
        Self { offset, size, data }
    }

    /// Returns the size in bytes of the replaced region.
    pub fn to_up_size(&self) -> u32 {
        self.size
    }

    /// Returns the byte offset at which the replacement starts.
    pub fn offset(&self) -> u32 {
        self.offset
    }

    /// Returns the replacement bytes.
    pub fn delta(&self) -> &Buf {
        &self.data
    }

    /// Applies this delta to `buf` and returns the compensation operation
    /// that undoes the change.
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

    /// Returns the size in bytes of the replaced region.
    pub fn to_replace_size(&self) -> usize {
        self.size as usize
    }

    /// Returns the byte offset at which the replacement starts.
    pub fn to_replace_start(&self) -> usize {
        self.offset as usize
    }

    /// Returns the byte offset at which the replacement ends.
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
        let mut data = vec![0; len];
        decoder.read_bytes(data.as_mut_slice())?;
        Ok(Self { offset, size, data })
    }
}

#[cfg(any(test, feature = "arbitrary"))]
impl Arbitrary<'_> for UpdateDelta {
    #[cfg(any(test, feature = "arbitrary"))]
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
        let uu = Unstructured::new(b);
        let data = Buf::arbitrary_take_rest(uu)?;
        Ok(Self { offset, size, data })
    }
}
