const CRC64XZ: crc::Crc<u64> = crc::Crc::<u64>::new(&crc::CRC_64_XZ);
const CRC32: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_ISCSI);
const CRC16: crc::Crc<u16> = crc::Crc::<u16>::new(&crc::CRC_16_IBM_SDLC);

pub fn calc_crc(bytes: &[u8]) -> u64 {
    CRC64XZ.checksum(bytes)
}

pub fn crc64(bytes: &[u8]) -> u64 {
    CRC64XZ.checksum(bytes)
}

pub fn crc32(bytes: &[u8]) -> u32 {
    CRC32.checksum(bytes)
}

pub fn crc16(bytes: &[u8]) -> u16 {
    CRC16.checksum(bytes)
}
