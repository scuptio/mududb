//! The slot-array entry that locates one record inside a page.

pub use super::record_slot_core::{
    RecordSlot, RECORD_SLOT_FLAG_FRAGMENT_MASK, RECORD_SLOT_FLAG_PARTIAL_CONTINUED,
    RECORD_SLOT_FLAG_PARTIAL_LAST,
};
pub(crate) use super::record_slot_core::{RECORD_SLOT_FLAG_COMPLETE, RECORD_SLOT_SIZE};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

impl RecordSlot {
    /// Decodes a slot from its 32-byte on-page encoding.
    pub fn decode(input: &[u8]) -> RS<Self> {
        if input.len() < RECORD_SLOT_SIZE {
            return Err(mudu_error!(
                ErrorCode::Decode,
                format!(
                    "record slot requires {} bytes, got {}",
                    RECORD_SLOT_SIZE,
                    input.len()
                )
            ));
        }

        Self::decode_bytes(input)
            .ok_or_else(|| mudu_error!(ErrorCode::Decode, "invalid record slot"))
    }

    /// Encodes the slot into `out` in its 32-byte on-page encoding.
    pub fn encode(&self, out: &mut [u8]) -> RS<()> {
        if out.len() < RECORD_SLOT_SIZE {
            return Err(mudu_error!(
                ErrorCode::Encode,
                format!(
                    "record slot encode requires {} bytes, got {}",
                    RECORD_SLOT_SIZE,
                    out.len()
                )
            ));
        }

        self.encode_bytes(out);
        Ok(())
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

    use super::{
        RecordSlot, RECORD_SLOT_FLAG_COMPLETE, RECORD_SLOT_FLAG_PARTIAL_CONTINUED,
        RECORD_SLOT_FLAG_PARTIAL_LAST, RECORD_SLOT_SIZE,
    };
    use crate::page::record_slot_ref::RecordSlotRef;

    #[test]
    fn record_slot_roundtrip() {
        let mut slot = RecordSlot::new(64, 23, 17, 29);
        slot.set_check_sum(31);
        slot.set_flag(37);
        slot.set_reserved(41);

        let mut encoded = [0u8; RECORD_SLOT_SIZE];
        slot.encode(&mut encoded).unwrap();
        let decoded = RecordSlot::decode(&encoded).unwrap();
        assert_eq!(decoded, slot);
    }

    #[test]
    fn record_slot_low_two_flag_bits_describe_fragment_kind() {
        let mut slot = RecordSlot::new(64, 23, 17, 29);
        slot.set_flag(0b1111_0000);
        slot.set_fragment_kind(RECORD_SLOT_FLAG_COMPLETE);
        assert!(slot.is_complete());
        assert_eq!(slot.flag(), 0b1111_0000);

        slot.set_fragment_kind(RECORD_SLOT_FLAG_PARTIAL_CONTINUED);
        assert!(slot.is_partial_continued());
        assert_eq!(slot.flag(), 0b1111_0001);

        slot.set_fragment_kind(RECORD_SLOT_FLAG_PARTIAL_LAST);
        assert!(slot.is_partial_last());
        assert_eq!(slot.flag(), 0b1111_0010);
    }

    #[test]
    fn record_slot_ref_reads_fields_without_full_decode() {
        let mut slot = RecordSlot::new(64, 23, 17, 29);
        slot.set_check_sum(31);
        slot.set_flag(37);
        slot.set_reserved(41);

        let mut encoded = [0u8; RECORD_SLOT_SIZE];
        slot.encode(&mut encoded).unwrap();
        let slot_ref = RecordSlotRef::new(&encoded).unwrap();
        assert_eq!(slot_ref.offset(), 64);
        assert_eq!(slot_ref.size(), 23);
        assert_eq!(slot_ref.timestamp(), 17);
        assert_eq!(slot_ref.tuple_id(), 29);
        assert_eq!(slot_ref.checksum(), 31);
        assert_eq!(slot_ref.flag(), 37);
        assert_eq!(slot_ref.reserved(), 41);
        assert_eq!(slot_ref.to_owned(), slot);
    }
    #[test]
    fn disk_fixture_and_short_buffers_preserve_all_bytes() {
        // Fixed historical little-endian layout, including unknown flag bits.
        let disk: [u8; 32] = std::array::from_fn(|i| i as u8);
        let slot = RecordSlot::decode(&disk).unwrap();
        assert_eq!(slot.offset(), 0x03020100);
        assert_eq!(slot.size(), 0x07060504);
        assert_eq!(slot.timestamp(), 0x0f0e0d0c0b0a0908);
        assert_eq!(slot.tuple_id(), 0x1716151413121110);
        assert_eq!(slot.check_sum(), 0x1918);
        assert_eq!(slot.flag(), 0x1b1a);
        assert_eq!(slot.reserved(), 0x1f1e1d1c);
        let mut out = [0xa5; 40];
        slot.encode(&mut out).unwrap();
        assert_eq!(&out[..32], &disk);
        assert_eq!(&out[32..], &[0xa5; 8]);
        for len in 0..32 {
            let mut short = vec![0xa5; len];
            assert!(slot.encode(&mut short).is_err());
            assert_eq!(short, vec![0xa5; len]);
            assert!(RecordSlot::decode(&disk[..len]).is_err());
        }
        for flag in 0..=u16::MAX {
            let mut changed = slot;
            changed.set_flag(flag);
            for kind in [0, 1, 2, 3, u16::MAX] {
                changed.set_fragment_kind(kind);
                assert_eq!(changed.flag(), (flag & !3) | (kind & 3));
                assert_eq!(changed.fragment_kind(), kind & 3);
                let mut bytes = [0; 32];
                changed.encode(&mut bytes).unwrap();
                assert_eq!(&bytes[..26], &disk[..26]);
                assert_eq!(&bytes[28..], &disk[28..]);
            }
        }
    }
}
