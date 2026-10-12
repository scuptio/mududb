//! The fixed-size trailer at the end of every page: the mirrored page LSN
//! and the whole-page checksum.

use ::wal::lsn::LSN;
use byteorder::{ByteOrder, LittleEndian};
use mudu::common::crc::crc32;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

pub(crate) const PAGE_TAILER_SIZE: usize = 12;

/// The page trailer: the LSN of the last page rewrite mirrored from the
/// header, and the CRC32 checksum over the whole page except this trailer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PageTailer {
    lsn: LSN,
    checksum: u32,
}

impl PageTailer {
    /// Creates a trailer from an LSN and a whole-page checksum.
    pub fn new(lsn: LSN, checksum: u32) -> Self {
        Self { lsn, checksum }
    }

    /// Decodes a trailer from the trailing bytes of a page.
    pub fn decode(input: &[u8]) -> RS<Self> {
        if input.len() < PAGE_TAILER_SIZE {
            return Err(mudu_error!(
                ErrorCode::Decode,
                format!(
                    "page tailer requires {} bytes, got {}",
                    PAGE_TAILER_SIZE,
                    input.len()
                )
            ));
        }

        Ok(Self {
            lsn: LittleEndian::read_u64(&input[0..8]).into(),
            checksum: LittleEndian::read_u32(&input[8..12]),
        })
    }

    /// Encodes the trailer into `out`.
    pub fn encode(&self, out: &mut [u8]) -> RS<()> {
        if out.len() < PAGE_TAILER_SIZE {
            return Err(mudu_error!(
                ErrorCode::Encode,
                format!(
                    "page tailer encode requires {} bytes, got {}",
                    PAGE_TAILER_SIZE,
                    out.len()
                )
            ));
        }

        LittleEndian::write_u64(&mut out[0..8], self.lsn.into());
        LittleEndian::write_u32(&mut out[8..12], self.checksum);
        Ok(())
    }

    /// Computes the page CRC32 over the full page except for the trailing tailer.
    /// The covered range is `[0, page.len() - PAGE_TAILER_SIZE)`.
    pub(crate) fn checksum_for_page(page: &[u8]) -> RS<u32> {
        if page.len() < PAGE_TAILER_SIZE {
            return Err(mudu_error!(
                ErrorCode::Encode,
                format!(
                    "page checksum requires at least {} bytes, got {}",
                    PAGE_TAILER_SIZE,
                    page.len()
                )
            ));
        }

        Ok(crc32(&page[..page.len() - PAGE_TAILER_SIZE]))
    }

    pub(crate) fn refresh_checksum(&mut self, page: &[u8]) -> RS<()> {
        self.checksum = Self::checksum_for_page(page)?;
        Ok(())
    }

    pub(crate) fn validate_checksum(&self, page: &[u8]) -> RS<()> {
        let actual = Self::checksum_for_page(page)?;
        if self.checksum != actual {
            return Err(mudu_error!(
                ErrorCode::Decode,
                format!(
                    "page checksum mismatch: stored={}, actual={}",
                    self.checksum, actual
                )
            ));
        }
        Ok(())
    }

    /// Returns the page LSN mirrored in the trailer.
    pub fn lsn(&self) -> LSN {
        self.lsn
    }

    /// Returns the stored whole-page checksum.
    pub fn checksum(&self) -> u32 {
        self.checksum
    }

    /// Sets the page LSN mirrored in the trailer.
    pub fn set_lsn(&mut self, lsn: LSN) {
        self.lsn = lsn;
    }

    /// Sets the stored whole-page checksum.
    pub fn set_checksum(&mut self, checksum: u32) {
        self.checksum = checksum;
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use super::{PageTailer, PAGE_TAILER_SIZE};
    use ::wal::lsn::LSN;

    #[test]
    fn page_tailer_encodes_to_fixed_12_bytes() {
        let tailer = PageTailer::default();
        let mut encoded = [0u8; PAGE_TAILER_SIZE];
        tailer.encode(&mut encoded).unwrap();
        assert_eq!(encoded.len(), PAGE_TAILER_SIZE);
    }

    #[test]
    fn page_tailer_roundtrip() {
        let tailer = PageTailer::new(LSN::new(17), 29);
        let mut encoded = [0u8; PAGE_TAILER_SIZE];
        tailer.encode(&mut encoded).unwrap();
        let decoded = PageTailer::decode(&encoded).unwrap();
        assert_eq!(decoded, tailer);
    }

    #[test]
    fn page_tailer_checksum_covers_page_except_tailer() {
        let mut page = [0u8; 36];
        page[..28].copy_from_slice(&[1u8; 28]);
        page[28..].copy_from_slice(&[9u8; 8]);

        let checksum = PageTailer::checksum_for_page(&page).unwrap();
        page[28..].copy_from_slice(&[7u8; 8]);
        assert_eq!(checksum, PageTailer::checksum_for_page(&page).unwrap());
    }
}
