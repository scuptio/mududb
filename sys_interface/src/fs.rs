//! Guest-side support for the `mudu_fs_*` filesystem syscall family.
//!
//! This module defines the public data types returned by the fs functions
//! ([`FsStat`] and [`FsDirEntry`]) and holds the encode/decode glue shared by
//! the synchronous and asynchronous implementations, keeping the wasm and
//! standalone impls as thin one-line forwarders. All frames are SyscallPayload
//! v1 (MSSP) frames produced by [`mudu_binding::codec::syscall_payload`].
//!
//! Errno mapping: the host reports argument errors with the application-level
//! code [`ErrorCode::InvalidArgument`] (50029); the guest-facing POSIX surface
//! maps it to `EINVAL` ([`ErrorCode::InvalidInput`], 22). The syscall frames
//! themselves still carry the host's original code.

use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::error::MuduError;
use mudu_binding::codec::syscall_payload;
use mudu_binding::universal::uni_fs_dirent::UniFsDirent;
use mudu_binding::universal::uni_fs_open_argv::UniFsOpenArgv;
use mudu_binding::universal::uni_fs_stat::UniFsStat;

/// Stat information returned by `mudu_fs_fstat` and `mudu_fs_stat`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsStat {
    /// Fs object OID.
    pub oid: OID,
    /// Generation the stat is anchored to.
    pub generation: u64,
    /// Object-relative entry path (empty for the object root).
    pub entry: String,
    /// Content length in bytes.
    pub length: u64,
    /// Object lifecycle state (0 = PENDING, 1 = SEALED).
    pub state: u32,
}

impl From<UniFsStat> for FsStat {
    fn from(stat: UniFsStat) -> Self {
        FsStat {
            oid: stat.oid.to_oid(),
            generation: stat.generation,
            entry: stat.entry,
            length: stat.length,
            state: stat.state,
        }
    }
}

/// Directory entry returned by `mudu_fs_readdir`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsDirEntry {
    /// Entry name (a single path segment).
    pub name: String,
    /// Whether the entry is a directory.
    pub is_dir: bool,
    /// Content length in bytes (0 for directories).
    pub length: u64,
}

impl From<UniFsDirent> for FsDirEntry {
    fn from(ent: UniFsDirent) -> Self {
        FsDirEntry {
            name: ent.name,
            is_dir: ent.is_dir,
            length: ent.length,
        }
    }
}

/// Map the host's application-level `InvalidArgument` code to the
/// POSIX-facing `EINVAL` (`InvalidInput`), keeping the original error as the
/// source so the host's message is preserved.
fn map_fs_errno(err: MuduError) -> MuduError {
    if err.ec() == ErrorCode::InvalidArgument {
        let msg = err.message().to_string();
        MuduError::new_with_ec_msg_src(ErrorCode::InvalidInput, msg, err)
    } else {
        err
    }
}

/// Serialize an `fs open` param frame.
pub(crate) fn serialize_fs_open_param(
    session_id: OID,
    oid: OID,
    path: &str,
    flags: u32,
) -> Vec<u8> {
    syscall_payload::encode_fs_open_request(&UniFsOpenArgv {
        session: session_id.into(),
        oid: oid.into(),
        path: path.to_string(),
        flags,
    })
}

/// Deserialize an `fs open` result frame.
pub(crate) fn deserialize_fs_open_result(input: &[u8]) -> RS<u32> {
    syscall_payload::decode_fs_open_result(input).map_err(map_fs_errno)
}

/// Serialize an `fs close` param frame.
pub(crate) fn serialize_fs_close_param(_session_id: OID, fd: u32) -> Vec<u8> {
    syscall_payload::encode_fs_close_request(fd)
}

/// Deserialize an `fs close` result frame.
pub(crate) fn deserialize_fs_close_result(input: &[u8]) -> RS<()> {
    syscall_payload::decode_fs_close_result(input).map_err(map_fs_errno)
}

/// Serialize an `fs read` param frame.
pub(crate) fn serialize_fs_read_param(_session_id: OID, fd: u32, len: u32) -> Vec<u8> {
    syscall_payload::encode_fs_read_request(fd, len)
}

/// Deserialize an `fs read` result frame.
pub(crate) fn deserialize_fs_read_result(input: &[u8]) -> RS<Vec<u8>> {
    syscall_payload::decode_fs_read_result(input).map_err(map_fs_errno)
}

/// Serialize an `fs write` param frame.
pub(crate) fn serialize_fs_write_param(_session_id: OID, fd: u32, data: &[u8]) -> Vec<u8> {
    syscall_payload::encode_fs_write_request(fd, data)
}

/// Deserialize an `fs write` result frame.
pub(crate) fn deserialize_fs_write_result(input: &[u8]) -> RS<u32> {
    syscall_payload::decode_fs_write_result(input).map_err(map_fs_errno)
}

/// Serialize an `fs pread` param frame.
pub(crate) fn serialize_fs_pread_param(
    _session_id: OID,
    fd: u32,
    offset: u64,
    len: u32,
) -> Vec<u8> {
    syscall_payload::encode_fs_pread_request(fd, offset, len)
}

/// Deserialize an `fs pread` result frame.
pub(crate) fn deserialize_fs_pread_result(input: &[u8]) -> RS<Vec<u8>> {
    syscall_payload::decode_fs_pread_result(input).map_err(map_fs_errno)
}

/// Serialize an `fs pwrite` param frame.
pub(crate) fn serialize_fs_pwrite_param(
    _session_id: OID,
    fd: u32,
    offset: u64,
    data: &[u8],
) -> Vec<u8> {
    syscall_payload::encode_fs_pwrite_request(fd, offset, data)
}

/// Deserialize an `fs pwrite` result frame.
pub(crate) fn deserialize_fs_pwrite_result(input: &[u8]) -> RS<()> {
    syscall_payload::decode_fs_pwrite_result(input).map_err(map_fs_errno)
}

/// Serialize an `fs lseek` param frame.
pub(crate) fn serialize_fs_lseek_param(
    _session_id: OID,
    fd: u32,
    offset: i64,
    whence: u32,
) -> Vec<u8> {
    syscall_payload::encode_fs_lseek_request(fd, offset, whence)
}

/// Deserialize an `fs lseek` result frame.
pub(crate) fn deserialize_fs_lseek_result(input: &[u8]) -> RS<u64> {
    syscall_payload::decode_fs_lseek_result(input).map_err(map_fs_errno)
}

/// Serialize an `fs fstat` param frame.
pub(crate) fn serialize_fs_fstat_param(_session_id: OID, fd: u32) -> Vec<u8> {
    syscall_payload::encode_fs_fstat_request(fd)
}

/// Deserialize an `fs fstat` result frame.
pub(crate) fn deserialize_fs_fstat_result(input: &[u8]) -> RS<FsStat> {
    syscall_payload::decode_fs_fstat_result(input)
        .map(FsStat::from)
        .map_err(map_fs_errno)
}

/// Serialize an `fs stat` param frame.
pub(crate) fn serialize_fs_stat_param(_session_id: OID, oid: OID, path: &str) -> Vec<u8> {
    syscall_payload::encode_fs_stat_request(oid.into(), path)
}

/// Deserialize an `fs stat` result frame.
pub(crate) fn deserialize_fs_stat_result(input: &[u8]) -> RS<FsStat> {
    syscall_payload::decode_fs_stat_result(input)
        .map(FsStat::from)
        .map_err(map_fs_errno)
}

/// Serialize an `fs fsync` param frame.
pub(crate) fn serialize_fs_fsync_param(_session_id: OID, fd: u32) -> Vec<u8> {
    syscall_payload::encode_fs_fsync_request(fd)
}

/// Deserialize an `fs fsync` result frame.
pub(crate) fn deserialize_fs_fsync_result(input: &[u8]) -> RS<()> {
    syscall_payload::decode_fs_fsync_result(input).map_err(map_fs_errno)
}

/// Serialize an `fs readdir` param frame.
pub(crate) fn serialize_fs_readdir_param(_session_id: OID, oid: OID, path: &str) -> Vec<u8> {
    syscall_payload::encode_fs_readdir_request(oid.into(), path)
}

/// Deserialize an `fs readdir` result frame.
pub(crate) fn deserialize_fs_readdir_result(input: &[u8]) -> RS<Vec<FsDirEntry>> {
    syscall_payload::decode_fs_readdir_result(input)
        .map(|ents| ents.into_iter().map(FsDirEntry::from).collect())
        .map_err(map_fs_errno)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use mudu::mudu_error;
    use mudu_binding::codec::syscall_payload::MessageKind;

    #[test]
    fn fs_stat_and_dir_entry_convert_from_universal_records() {
        let stat = FsStat::from(UniFsStat {
            oid: 7u128.into(),
            generation: 3,
            entry: "docs/a.txt".to_string(),
            length: 42,
            state: 1,
        });
        assert_eq!(
            stat,
            FsStat {
                oid: 7,
                generation: 3,
                entry: "docs/a.txt".to_string(),
                length: 42,
                state: 1,
            }
        );

        let entry = FsDirEntry::from(UniFsDirent {
            name: "docs".to_string(),
            is_dir: true,
            length: 0,
        });
        assert_eq!(
            entry,
            FsDirEntry {
                name: "docs".to_string(),
                is_dir: true,
                length: 0,
            }
        );
    }

    #[test]
    fn fs_open_param_roundtrips_through_payload_codec() {
        let encoded = serialize_fs_open_param(11, 22, "docs/a.txt", 2);
        let (kind, _) = syscall_payload::decode_frame(&encoded).unwrap();
        assert_eq!(kind, MessageKind::FsOpen);
        let decoded = syscall_payload::decode_fs_open_request(&encoded).unwrap();
        assert_eq!(decoded.session.to_oid(), 11);
        assert_eq!(decoded.oid.to_oid(), 22);
        assert_eq!(decoded.path, "docs/a.txt");
        assert_eq!(decoded.flags, 2);

        let fd =
            deserialize_fs_open_result(&syscall_payload::encode_fs_open_result(&Ok(9))).unwrap();
        assert_eq!(fd, 9);
    }

    #[test]
    fn fs_param_frames_drop_the_session_from_the_wire() {
        // fd-based and oid/path-based fs requests do not carry the session oid
        // in the SyscallPayload v1 wire shape.
        let (kind, _) = syscall_payload::decode_frame(&serialize_fs_close_param(1, 3)).unwrap();
        assert_eq!(kind, MessageKind::FsClose);
        assert_eq!(
            syscall_payload::decode_fs_close_request(&serialize_fs_close_param(1, 3)).unwrap(),
            3
        );

        let (stat_oid, stat_path) =
            syscall_payload::decode_fs_stat_request(&serialize_fs_stat_param(1, 22, "a")).unwrap();
        assert_eq!(stat_oid.to_oid(), 22);
        assert_eq!(stat_path, "a");

        let (readdir_oid, readdir_path) =
            syscall_payload::decode_fs_readdir_request(&serialize_fs_readdir_param(1, 23, "d"))
                .unwrap();
        assert_eq!(readdir_oid.to_oid(), 23);
        assert_eq!(readdir_path, "d");
    }

    #[test]
    fn fs_stat_result_decodes_into_fs_stat() {
        let stat = UniFsStat {
            oid: 5u128.into(),
            generation: 1,
            entry: String::new(),
            length: 100,
            state: 1,
        };
        let stat = deserialize_fs_fstat_result(&syscall_payload::encode_fs_fstat_result(&Ok(stat)))
            .unwrap();
        assert_eq!(stat.oid, 5);
        assert_eq!(stat.length, 100);
    }

    #[test]
    fn fs_readdir_result_decodes_into_dir_entries() {
        let encoded = syscall_payload::encode_fs_readdir_result(&Ok(vec![
            UniFsDirent {
                name: "a.txt".to_string(),
                is_dir: false,
                length: 3,
            },
            UniFsDirent {
                name: "docs".to_string(),
                is_dir: true,
                length: 0,
            },
        ]));
        let entries = deserialize_fs_readdir_result(&encoded).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "a.txt");
        assert!(!entries[0].is_dir);
        assert_eq!(entries[0].length, 3);
        assert!(entries[1].is_dir);
    }

    #[test]
    fn invalid_argument_maps_to_invalid_input() {
        let host_err = mudu_error!(
            ErrorCode::InvalidArgument,
            "unsupported fs open flags 0o101"
        );
        let encoded = syscall_payload::encode_fs_open_result(&Err(host_err));
        let err = deserialize_fs_open_result(&encoded).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidInput);
        assert!(err.message().contains("unsupported fs open flags"));
    }

    #[test]
    fn other_error_codes_pass_through_unchanged() {
        let host_err = mudu_error!(
            ErrorCode::BadFileDescriptor,
            "fs fd 3 is not open for reading"
        );
        let encoded = syscall_payload::encode_fs_read_result(&Err(host_err));
        let err = deserialize_fs_read_result(&encoded).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);
    }
}
