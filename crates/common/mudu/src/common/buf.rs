pub type Buf = Vec<u8>;

pub fn resize_buf(buf: &mut Buf, size: usize) {
    buf.resize(size, 0);
}
