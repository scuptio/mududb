#![allow(clippy::unwrap_used)]
#![allow(deprecated)]

use super::*;
use crate::codec::handle_sys_session::serialize_error_result;
use mudu::error::ErrorCode;

const TEST_SESSION: OID = 0x1234_5678_90ab_cdef_1234_5678_90ab_cdefu128;
const TEST_OID: OID = 0xfedc_ba09_8765_4321_fedc_ba09_8765_4321u128;

fn test_stat_frame() -> FsStatFrame {
    FsStatFrame {
        oid: TEST_OID,
        generation: 7,
        entry: "dir/file.dat".to_string(),
        length: 4096,
        state: 1,
    }
}

#[test]
fn fs_open_param_roundtrip_and_truncation() {
    let param = FsOpenParam {
        session_id: TEST_SESSION,
        oid: TEST_OID,
        path: "a/b.txt".to_string(),
        flags: 0o2,
    };
    let payload = serialize_fs_open_param(&param);
    assert_eq!(deserialize_fs_open_param(&payload).unwrap(), param);

    let err = deserialize_fs_open_param(&payload[..10]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let err = deserialize_fs_open_param(&payload[..size_of::<u128>()]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let err = deserialize_fs_open_param(&payload[..payload.len() - 1]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);
}

#[test]
fn fs_open_result_roundtrip_and_error() {
    let payload = serialize_fs_open_result(42);
    assert_eq!(deserialize_fs_open_result(&payload).unwrap(), 42);

    let err = deserialize_fs_open_result(&payload[..2]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let payload = serialize_error_result(mudu::mudu_error!(ErrorCode::NotFound, "no such file"));
    let err = deserialize_fs_open_result(&payload).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::NotFound);
}

#[test]
fn fs_close_param_roundtrip_and_truncation() {
    let payload = serialize_fs_close_param(TEST_SESSION, 3);
    let (sid, fd) = deserialize_fs_close_param(&payload).unwrap();
    assert_eq!(sid, TEST_SESSION);
    assert_eq!(fd, 3);

    let err = deserialize_fs_close_param(&payload[..10]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let err = deserialize_fs_close_param(&payload[..payload.len() - 1]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);
}

#[test]
fn fs_close_result_ok_invalid_and_error() {
    let ok = serialize_fs_close_result();
    assert!(deserialize_fs_close_result(&ok).is_ok());

    let err = deserialize_fs_close_result(&[2]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let payload = serialize_error_result(mudu::mudu_error!(ErrorCode::BadFileDescriptor, "bad fd"));
    let err = deserialize_fs_close_result(&payload).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);
}

#[test]
fn fs_read_param_roundtrip_and_truncation() {
    let payload = serialize_fs_read_param(TEST_SESSION, 5, 1024);
    let (sid, fd, len) = deserialize_fs_read_param(&payload).unwrap();
    assert_eq!(sid, TEST_SESSION);
    assert_eq!(fd, 5);
    assert_eq!(len, 1024);

    let err = deserialize_fs_read_param(&payload[..10]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let err = deserialize_fs_read_param(&payload[..payload.len() - 1]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);
}

#[test]
fn fs_read_result_roundtrip_and_error() {
    let data = b"hello fs";
    let payload = serialize_fs_read_result(data);
    assert_eq!(deserialize_fs_read_result(&payload).unwrap(), data);

    let empty = serialize_fs_read_result(&[]);
    assert!(deserialize_fs_read_result(&empty).unwrap().is_empty());

    let err = deserialize_fs_read_result(&payload[..2]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let err = deserialize_fs_read_result(&payload[..payload.len() - 2]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let payload = serialize_error_result(mudu::mudu_error!(ErrorCode::Internal, "read failed"));
    let err = deserialize_fs_read_result(&payload).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Internal);
}

#[test]
fn fs_write_param_roundtrip_and_truncation() {
    let data = b"payload bytes";
    let payload = serialize_fs_write_param(TEST_SESSION, 9, data);
    let (sid, fd, got_data) = deserialize_fs_write_param(&payload).unwrap();
    assert_eq!(sid, TEST_SESSION);
    assert_eq!(fd, 9);
    assert_eq!(got_data, data);

    let err = deserialize_fs_write_param(&payload[..10]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let err = deserialize_fs_write_param(&payload[..payload.len() - 2]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);
}

#[test]
fn fs_write_result_roundtrip_and_error() {
    let payload = serialize_fs_write_result(512);
    assert_eq!(deserialize_fs_write_result(&payload).unwrap(), 512);

    let err = deserialize_fs_write_result(&payload[..2]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let payload = serialize_error_result(mudu::mudu_error!(ErrorCode::WriteZero, "write failed"));
    let err = deserialize_fs_write_result(&payload).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::WriteZero);
}

#[test]
fn fs_pread_param_roundtrip_and_truncation() {
    let payload = serialize_fs_pread_param(TEST_SESSION, 5, 1 << 40, 256);
    let (sid, fd, offset, len) = deserialize_fs_pread_param(&payload).unwrap();
    assert_eq!(sid, TEST_SESSION);
    assert_eq!(fd, 5);
    assert_eq!(offset, 1 << 40);
    assert_eq!(len, 256);

    let err = deserialize_fs_pread_param(&payload[..20]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let err = deserialize_fs_pread_param(&payload[..payload.len() - 1]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);
}

#[test]
fn fs_pread_result_roundtrip_and_error() {
    let data = b"pread data";
    let payload = serialize_fs_pread_result(data);
    assert_eq!(deserialize_fs_pread_result(&payload).unwrap(), data);

    let err = deserialize_fs_pread_result(&payload[..payload.len() - 2]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let payload =
        serialize_error_result(mudu::mudu_error!(ErrorCode::InvalidArgument, "bad offset"));
    let err = deserialize_fs_pread_result(&payload).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidArgument);
}

#[test]
fn fs_pwrite_param_roundtrip_and_truncation() {
    let data = b"pwrite data";
    let payload = serialize_fs_pwrite_param(TEST_SESSION, 6, 1 << 33, data);
    let (sid, fd, offset, got_data) = deserialize_fs_pwrite_param(&payload).unwrap();
    assert_eq!(sid, TEST_SESSION);
    assert_eq!(fd, 6);
    assert_eq!(offset, 1 << 33);
    assert_eq!(got_data, data);

    let err = deserialize_fs_pwrite_param(&payload[..20]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let err = deserialize_fs_pwrite_param(&payload[..payload.len() - 2]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);
}

#[test]
fn fs_pwrite_result_ok_invalid_and_error() {
    let ok = serialize_fs_pwrite_result();
    assert!(deserialize_fs_pwrite_result(&ok).is_ok());

    let err = deserialize_fs_pwrite_result(&[2]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let payload = serialize_error_result(mudu::mudu_error!(
        ErrorCode::PermissionDenied,
        "read-only generation"
    ));
    let err = deserialize_fs_pwrite_result(&payload).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::PermissionDenied);
}

#[test]
fn fs_lseek_param_roundtrip_and_truncation() {
    let payload = serialize_fs_lseek_param(TEST_SESSION, 4, -128, 2);
    let (sid, fd, offset, whence) = deserialize_fs_lseek_param(&payload).unwrap();
    assert_eq!(sid, TEST_SESSION);
    assert_eq!(fd, 4);
    assert_eq!(offset, -128);
    assert_eq!(whence, 2);

    let err = deserialize_fs_lseek_param(&payload[..20]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let err = deserialize_fs_lseek_param(&payload[..payload.len() - 1]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);
}

#[test]
fn fs_lseek_result_roundtrip_and_error() {
    let payload = serialize_fs_lseek_result(1 << 50);
    assert_eq!(deserialize_fs_lseek_result(&payload).unwrap(), 1 << 50);

    let err = deserialize_fs_lseek_result(&payload[..4]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let payload = serialize_error_result(mudu::mudu_error!(
        ErrorCode::InvalidArgument,
        "negative cursor"
    ));
    let err = deserialize_fs_lseek_result(&payload).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidArgument);
}

#[test]
fn fs_fstat_param_roundtrip_and_truncation() {
    let payload = serialize_fs_fstat_param(TEST_SESSION, 8);
    let (sid, fd) = deserialize_fs_fstat_param(&payload).unwrap();
    assert_eq!(sid, TEST_SESSION);
    assert_eq!(fd, 8);

    let err = deserialize_fs_fstat_param(&payload[..10]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let err = deserialize_fs_fstat_param(&payload[..payload.len() - 1]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);
}

#[test]
fn fs_fstat_result_roundtrip_and_error() {
    let stat = test_stat_frame();
    let payload = serialize_fs_fstat_result(&stat);
    assert_eq!(deserialize_fs_fstat_result(&payload).unwrap(), stat);

    let err = deserialize_fs_fstat_result(&payload[..10]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let err = deserialize_fs_fstat_result(&payload[..payload.len() - 2]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let payload = serialize_error_result(mudu::mudu_error!(ErrorCode::BadFileDescriptor, "bad fd"));
    let err = deserialize_fs_fstat_result(&payload).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);
}

#[test]
fn fs_stat_param_roundtrip_and_truncation() {
    let payload = serialize_fs_stat_param(TEST_SESSION, TEST_OID, "sub/dir");
    let (sid, oid, path) = deserialize_fs_stat_param(&payload).unwrap();
    assert_eq!(sid, TEST_SESSION);
    assert_eq!(oid, TEST_OID);
    assert_eq!(path, "sub/dir");

    let err = deserialize_fs_stat_param(&payload[..10]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let err = deserialize_fs_stat_param(&payload[..payload.len() - 1]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);
}

#[test]
fn fs_stat_result_roundtrip_and_error() {
    let stat = test_stat_frame();
    let payload = serialize_fs_stat_result(&stat);
    assert_eq!(deserialize_fs_stat_result(&payload).unwrap(), stat);

    let empty_entry = FsStatFrame {
        entry: String::new(),
        ..test_stat_frame()
    };
    let payload = serialize_fs_stat_result(&empty_entry);
    assert_eq!(deserialize_fs_stat_result(&payload).unwrap(), empty_entry);

    let err = deserialize_fs_stat_result(&payload[..payload.len() - 2]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let payload = serialize_error_result(mudu::mudu_error!(ErrorCode::NotFound, "no such entry"));
    let err = deserialize_fs_stat_result(&payload).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::NotFound);
}

#[test]
fn fs_fsync_param_roundtrip_and_truncation() {
    let payload = serialize_fs_fsync_param(TEST_SESSION, 11);
    let (sid, fd) = deserialize_fs_fsync_param(&payload).unwrap();
    assert_eq!(sid, TEST_SESSION);
    assert_eq!(fd, 11);

    let err = deserialize_fs_fsync_param(&payload[..10]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let err = deserialize_fs_fsync_param(&payload[..payload.len() - 1]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);
}

#[test]
fn fs_fsync_result_ok_invalid_and_error() {
    let ok = serialize_fs_fsync_result();
    assert!(deserialize_fs_fsync_result(&ok).is_ok());

    let err = deserialize_fs_fsync_result(&[2]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let payload = serialize_error_result(mudu::mudu_error!(ErrorCode::Internal, "fsync failed"));
    let err = deserialize_fs_fsync_result(&payload).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Internal);
}

#[test]
fn fs_readdir_param_roundtrip_and_truncation() {
    let payload = serialize_fs_readdir_param(TEST_SESSION, TEST_OID, "");
    let (sid, oid, path) = deserialize_fs_readdir_param(&payload).unwrap();
    assert_eq!(sid, TEST_SESSION);
    assert_eq!(oid, TEST_OID);
    assert_eq!(path, "");

    let payload = serialize_fs_readdir_param(TEST_SESSION, TEST_OID, "docs");
    let (sid, oid, path) = deserialize_fs_readdir_param(&payload).unwrap();
    assert_eq!(sid, TEST_SESSION);
    assert_eq!(oid, TEST_OID);
    assert_eq!(path, "docs");

    let err = deserialize_fs_readdir_param(&payload[..10]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let err = deserialize_fs_readdir_param(&payload[..payload.len() - 1]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);
}

#[test]
fn fs_readdir_result_roundtrip_and_error() {
    let entries = vec![
        FsDirEnt {
            name: "a.txt".to_string(),
            is_dir: false,
            length: 12,
        },
        FsDirEnt {
            name: "sub".to_string(),
            is_dir: true,
            length: 0,
        },
        FsDirEnt {
            name: "longer-entry-name.bin".to_string(),
            is_dir: false,
            length: 1 << 20,
        },
    ];
    let payload = serialize_fs_readdir_result(&entries);
    assert_eq!(deserialize_fs_readdir_result(&payload).unwrap(), entries);

    let empty: Vec<FsDirEnt> = Vec::new();
    let payload = serialize_fs_readdir_result(&empty);
    assert!(deserialize_fs_readdir_result(&payload).unwrap().is_empty());

    let payload = serialize_fs_readdir_result(&entries);
    let err = deserialize_fs_readdir_result(&payload[..payload.len() - 2]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let mut bad_flag = serialize_fs_readdir_result(&entries[..1]);
    let flag_index = 4 + 4 + "a.txt".len();
    bad_flag[flag_index] = 2;
    let err = deserialize_fs_readdir_result(&bad_flag).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let payload = serialize_error_result(mudu::mudu_error!(
        ErrorCode::NotADirectory,
        "not a directory"
    ));
    let err = deserialize_fs_readdir_result(&payload).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::NotADirectory);
}
