use md5::Digest;

pub fn calc_md5(bytes: &[u8]) -> String {
    let mut md5 = md5::Md5::new();
    md5.update(bytes);
    let s = md5.finalize();
    hex::encode(s)
}
