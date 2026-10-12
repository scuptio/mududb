use super::syscall_v1::GET_REQUEST;
use super::*;

/// Verifies structured compatibility errors for corrupt or unknown inputs.
#[test]
fn corruption_rejects_bad_magic_version_and_truncation() {
    page_header_corruption_cases();
    log_frame_corruption_cases();
    protocol_frame_corruption_cases();
    syscall_header_corruption_cases();
}

/// Page header v1: bad magic, unsupported version, truncation.
fn page_header_corruption_cases() {
    // Bad page header magic.
    let mut bad_page =
        mudu_sys::fs::sync::sync_read_all(fixture_path("page_header_v1.bin")).unwrap();
    bad_page[0] ^= 0xFF;
    let err = PageHeader::decode(&bad_page).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::CorruptedData);
    assert!(err.message().contains("invalid page magic"));

    // Unsupported page header version.
    let mut bad_version =
        mudu_sys::fs::sync::sync_read_all(fixture_path("page_header_v1.bin")).unwrap();
    bad_version[4..8].copy_from_slice(&2u32.to_le_bytes());
    let err = PageHeader::decode(&bad_version).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::UnsupportedFormatVersion);
    assert!(err.message().contains("unsupported page version"));

    // Truncated page header.
    let truncated =
        &mudu_sys::fs::sync::sync_read_all(fixture_path("page_header_v1.bin")).unwrap()[..64];
    let err = PageHeader::decode(truncated).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::Decode);
}

/// Log frame v1: bad magic, unsupported version.
fn log_frame_corruption_cases() {
    // Bad log frame magic.
    let mut bad_log = mudu_sys::fs::sync::sync_read_all(fixture_path("log_frame_v1.bin")).unwrap();
    bad_log[0] ^= 0xFF;
    let err = ::wal::format::latest::frame_lsn(&bad_log).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::CorruptedData);
    assert!(err.message().contains("invalid log frame magic"));

    // Unsupported log frame version.
    let mut bad_log_version =
        mudu_sys::fs::sync::sync_read_all(fixture_path("log_frame_v1.bin")).unwrap();
    bad_log_version[4..8].copy_from_slice(&99u32.to_be_bytes());
    let err = ::wal::format::latest::frame_lsn(&bad_log_version).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::UnsupportedFormatVersion);
    assert!(err.message().contains("unsupported log frame version"));
}

/// Protocol frame v1: bad magic, unsupported version, truncation.
fn protocol_frame_corruption_cases() {
    // Bad protocol frame magic.
    let mut bad_proto =
        mudu_sys::fs::sync::sync_read_all(fixture_path("protocol_frame_v1.bin")).unwrap();
    bad_proto[0] ^= 0xFF;
    let err = Frame::decode(&bad_proto).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::CorruptedData);
    assert!(err.message().contains("invalid protocol frame magic"));

    // Unsupported protocol frame version.
    let mut bad_proto_version =
        mudu_sys::fs::sync::sync_read_all(fixture_path("protocol_frame_v1.bin")).unwrap();
    bad_proto_version[4..8].copy_from_slice(&99u32.to_be_bytes());
    let err = Frame::decode(&bad_proto_version).unwrap_err();
    assert_eq!(
        err.ec(),
        mudu::error::ErrorCode::IncompatibleProtocolVersion
    );
    assert!(err.message().contains("unsupported protocol frame version"));

    // Truncated protocol frame.
    let truncated =
        &mudu_sys::fs::sync::sync_read_all(fixture_path("protocol_frame_v1.bin")).unwrap()[..20];
    let err = Frame::decode(truncated).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::Parse);
}

/// SyscallPayload v1 (MSSP) header integrity: bad magic, unsupported version,
/// nonzero flags, unknown kind, truncated header.
fn syscall_header_corruption_cases() {
    let syscall_bytes =
        mudu_sys::fs::sync::sync_read_all(fixture_path("syscall_payload_v1.bin")).unwrap();
    let segments = unpack_syscall_segments(&syscall_bytes);
    let get_frame = segments[GET_REQUEST];

    // Bad syscall payload magic.
    let mut bad_magic = get_frame.to_vec();
    bad_magic[0] ^= 0xFF;
    let err = decode_header(&bad_magic).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::CorruptedData);
    assert!(err.message().contains("invalid syscall payload magic"));

    // Unsupported syscall payload version.
    let mut bad_version = get_frame.to_vec();
    bad_version[4..8].copy_from_slice(&2u32.to_be_bytes());
    let err = decode_header(&bad_version).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::UnsupportedFormatVersion);
    assert!(
        err.message()
            .contains("unsupported syscall payload version")
    );

    // Nonzero header flags.
    let mut bad_flags = get_frame.to_vec();
    bad_flags[8..12].copy_from_slice(&1u32.to_be_bytes());
    let err = decode_header(&bad_flags).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::CorruptedData);
    assert!(err.message().contains("nonzero header flags"));

    // Kind 0 and unknown kind.
    for bad_kind in [0u32, 99] {
        let mut bad_kind_frame = get_frame.to_vec();
        bad_kind_frame[12..16].copy_from_slice(&bad_kind.to_be_bytes());
        let err = decode_header(&bad_kind_frame).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Decode);
        assert!(err.message().contains("unknown syscall message kind"));
    }

    // Truncated syscall payload header.
    let err = decode_header(&get_frame[..HEADER_LEN - 1]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::CorruptedData);
    assert!(err.message().contains("header shorter than 16 bytes"));
}
