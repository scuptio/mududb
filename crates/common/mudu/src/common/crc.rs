//! CRC checksum helpers.

const CRC64XZ: crc::Crc<u64> = crc::Crc::<u64>::new(&crc::CRC_64_XZ);
const CRC16: crc::Crc<u16> = crc::Crc::<u16>::new(&crc::CRC_16_IBM_SDLC);

/// Computes the CRC-64/XZ checksum of `bytes`.
pub fn calc_crc(bytes: &[u8]) -> u64 {
    CRC64XZ.checksum(bytes)
}

/// Computes the CRC-64/XZ checksum of `bytes`.
pub fn crc64(bytes: &[u8]) -> u64 {
    CRC64XZ.checksum(bytes)
}

/// Computes the CRC-32/ISCSI (CRC-32C) checksum of `bytes`.
/// Hardware dispatch preserves the exact result of the portable implementation.
pub fn crc32(bytes: &[u8]) -> u32 {
    crc32c::crc32c(bytes)
}

/// Incremental CRC-64/XZ state for discontiguous data, without concatenation.
pub struct Crc64 {
    digest: crc::Digest<'static, u64>,
}

impl Crc64 {
    /// Starts a checksum with the same initialization as [`crc64`].
    pub fn new() -> Self {
        Self {
            digest: CRC64XZ.digest(),
        }
    }

    /// Appends bytes in their logical order; empty slices leave the state unchanged.
    pub fn update(&mut self, bytes: &[u8]) {
        self.digest.update(bytes);
    }

    /// Finishes the checksum, applying CRC-64/XZ's final transformation once.
    pub fn finalize(self) -> u64 {
        self.digest.finalize()
    }
}

impl Default for Crc64 {
    fn default() -> Self {
        Self::new()
    }
}

/// Computes the CRC-16/IBM-SDLC checksum of `bytes`.
pub fn crc16(bytes: &[u8]) -> u16 {
    CRC16.checksum(bytes)
}
