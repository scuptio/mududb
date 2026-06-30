use crate::common::codec::{DecErr, Decoder, EncErr, Encoder};
pub struct SliceRef<'r> {
    s: &'r [u8],
    read_pos: usize,
}

pub struct SliceMutRef<'r> {
    s: &'r mut [u8],
    write_pos: usize,
}

impl<'r> SliceMutRef<'r> {
    pub fn new(s: &'r mut [u8]) -> Self {
        Self { s, write_pos: 0 }
    }
    pub fn capacity(&self) -> usize {
        self.s.len()
    }

    pub fn write_pos(&self) -> usize {
        self.write_pos
    }

    pub fn set_write_pos(&mut self) {
        self.write_pos = 0;
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.s[..self.write_pos]
    }
}

impl<'r> SliceRef<'r> {
    pub fn new(s: &'r [u8]) -> Self {
        Self { s, read_pos: 0 }
    }

    pub fn read_pos(&self) -> usize {
        self.read_pos
    }

    pub fn resize(&mut self) {
        self.read_pos = 0;
    }

    pub fn as_slice(&self) -> &'r [u8] {
        &self.s[..self.read_pos]
    }
}

impl Encoder for SliceMutRef<'_> {
    fn write(&mut self, bytes: &[u8]) -> Result<(), EncErr> {
        if self.s.len() >= self.write_pos + bytes.len() {
            self.s[self.write_pos..self.write_pos + bytes.len()].copy_from_slice(bytes);
            self.write_pos += bytes.len();
            Ok(())
        } else {
            Err(EncErr::CapacityNotAvailable)
        }
    }
}

impl Decoder for SliceRef<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), DecErr> {
        if self.s.len() >= self.read_pos + bytes.len() {
            bytes.copy_from_slice(&self.s[self.read_pos..self.read_pos + bytes.len()]);
            self.read_pos += bytes.len();
            Ok(())
        } else {
            Err(DecErr::CapacityNotAvailable)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SliceMutRef, SliceRef};
    use crate::common::codec::{Decoder, Encoder};

    #[test]
    fn slice_mut_ref_tracks_written_bytes() {
        let mut buf = [0_u8; 6];
        let mut writer = SliceMutRef::new(&mut buf);

        writer.write(b"ab").unwrap();
        writer.write(b"cd").unwrap();

        assert_eq!(writer.capacity(), 6);
        assert_eq!(writer.write_pos(), 4);
        assert_eq!(writer.as_slice(), b"abcd");
    }

    #[test]
    fn slice_ref_reads_incrementally_and_resize_resets_cursor() {
        let mut reader = SliceRef::new(b"wxyz");
        let mut buf = [0_u8; 2];

        reader.read(&mut buf).unwrap();
        assert_eq!(&buf, b"wx");
        assert_eq!(reader.read_pos(), 2);
        assert_eq!(reader.as_slice(), b"wx");

        reader.resize();
        assert_eq!(reader.read_pos(), 0);
        assert_eq!(reader.as_slice(), b"");
    }

    #[test]
    fn slice_mut_ref_write_returns_capacity_error() {
        let mut buf = [0_u8; 2];
        let mut writer = SliceMutRef::new(&mut buf);
        assert!(writer.write(b"abc").is_err());
    }

    #[test]
    fn slice_mut_ref_resets_write_position() {
        let mut buf = [0_u8; 4];
        let mut writer = SliceMutRef::new(&mut buf);
        writer.write(b"ab").unwrap();
        writer.set_write_pos();
        assert_eq!(writer.write_pos(), 0);
        assert_eq!(writer.as_slice(), b"");
    }

    #[test]
    fn slice_ref_read_returns_capacity_error() {
        let mut reader = SliceRef::new(b"a");
        let mut buf = [0_u8; 2];
        assert!(reader.read(&mut buf).is_err());
    }
}
