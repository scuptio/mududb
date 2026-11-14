use super::mssp_frame::*;

const MSSP_MAGIC: u32 = 0x4d53_5350;

#[test]
fn header_layout_exact_bytes() {
    let header = encode_header_fields(MSSP_MAGIC, 1, 7);
    assert_eq!(
        header,
        [
            0x4d, 0x53, 0x53, 0x50, // magic "MSSP"
            0x00, 0x00, 0x00, 0x01, // version
            0x00, 0x00, 0x00, 0x00, // flags
            0x00, 0x00, 0x00, 0x07, // message kind
        ]
    );
}

#[test]
fn header_fields_roundtrip_over_appended_body() {
    let mut frame = encode_header_fields(MSSP_MAGIC, 1, 23).to_vec();
    frame.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
    assert_eq!(decode_header_fields(&frame), Some((MSSP_MAGIC, 1, 0, 23)));
}

#[test]
fn header_fields_preserve_reserved_flags_on_decode() {
    let mut header = encode_header_fields(MSSP_MAGIC, 1, 1);
    header[11] = 0xab;
    assert_eq!(
        decode_header_fields(&header),
        Some((MSSP_MAGIC, 1, 0xab, 1))
    );
}

#[test]
fn short_inputs_are_rejected() {
    assert_eq!(decode_header_fields(&[]), None);
    assert_eq!(
        decode_header_fields(&encode_header_fields(MSSP_MAGIC, 1, 1)[..HEADER_LEN - 1]),
        None
    );
}
