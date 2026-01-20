use crate::common::endian::{read_u32, write_u32};

pub fn write_sized_buf(dest: &mut [u8], src: &[u8]) -> u32 {
    let len_bytes = size_of::<u32>();
    if dest.len() < len_bytes + src.len() {
        0
    } else {
        write_u32(dest, src.len() as u32);
        dest[len_bytes..len_bytes + src.len()].copy_from_slice(src);
        src.len() as u32 + len_bytes as u32
    }
}

pub fn read_sized_buf(buf: &[u8]) -> Result<(u32, &[u8]), Option<u32>> {
    let len_bytes = size_of::<u32>();
    if buf.len() < len_bytes {
        return Err(None);
    }
    let n = read_u32(buf);
    if n as usize + len_bytes < buf.len() {
        return Err(Some(n));
    }
    Ok((
        n + size_of::<u32>() as u32,
        &buf[len_bytes..len_bytes + n as usize],
    ))
}
