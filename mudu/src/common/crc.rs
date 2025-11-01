use crc::{CRC_64_XZ, Crc};
const CRC64XZ: Crc<u64> = Crc::<u64>::new(&CRC_64_XZ);

pub fn calc_crc(bytes: &[u8]) -> u64 {
    CRC64XZ.checksum(bytes)
}
