use md5::Digest;

pub fn calc_md5(bytes: &[u8]) -> String {
    let mut md5 = md5::Md5::new();
    md5.update(bytes);
    let s = md5.finalize();
    hex::encode(s)
}

#[cfg(test)]
mod tests {
    use super::calc_md5;

    #[test]
    fn calc_md5_empty() {
        assert_eq!(calc_md5(b""), "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn calc_md5_hello() {
        assert_eq!(calc_md5(b"hello"), "5d41402abc4b2a76b9719d911017c592");
    }

    #[test]
    fn calc_md5_format_is_lowercase_hex_32() {
        let hash = calc_md5(b"any input");
        assert_eq!(hash.len(), 32);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(hash, hash.to_ascii_lowercase());
    }
}
