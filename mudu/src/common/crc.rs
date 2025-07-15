const CRC64XZ: crc::Crc<u64> = crc::Crc::<u64>::new(&crc::CRC_64_XZ);

pub fn calc_crc(bytes: &[u8]) -> u64 {
    CRC64XZ.checksum(bytes)
}
