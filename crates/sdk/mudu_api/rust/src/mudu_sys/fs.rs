//! Frame codecs for the `mudu_fs_*` filesystem syscall family.
//!
//! Thin adapter over the `mudu_gen`-generated codec in
//! [`super::generated::uni_syscall`]: requests are positional MessagePack
//! arrays (byte blobs as bin), results are `[0, value]` / `[1, UniError]`
//! with unit results as `[0, 0]`.
//!
//! Errno mapping mirrors `sys_interface::fs::map_fs_errno`: the host reports
//! argument errors with the application-level code 50029
//! (`ErrorCode::InvalidArgument`); the guest-facing POSIX surface maps it to
//! `EINVAL` (22, `ErrorCode::InvalidInput`). The frames themselves still
//! carry the host's original code.

use super::generated::uni_syscall as gen_codec;
use super::payload;
use crate::error::ApiError;
use crate::types::UniReturn;
use crate::universal::uni_error::UniError;
use crate::universal::uni_fs_dirent::UniFsDirent;
use crate::universal::uni_fs_open_argv::UniFsOpenArgv;
use crate::universal::uni_fs_stat::UniFsStat;
use crate::universal::uni_oid::UniOid;

/// Host application-level `InvalidArgument` error code reported on the wire.
pub const FS_ERR_INVALID_ARGUMENT: u32 = 50029;

/// Guest-facing POSIX `EINVAL` (`InvalidInput`) code it maps to.
pub const FS_ERR_INVALID_INPUT: u32 = 22;

/// Maps the host's application-level `InvalidArgument` code in an fs result
/// to the POSIX-facing `EINVAL`, keeping the original message; every other
/// code passes through unchanged.
pub fn map_fs_errno<T>(result: UniReturn<T>) -> UniReturn<T> {
    result.map_err(|error| {
        if error.err_code == FS_ERR_INVALID_ARGUMENT {
            UniError {
                err_code: FS_ERR_INVALID_INPUT,
                ..error
            }
        } else {
            error
        }
    })
}

// ---- fs-open ----

/// Serializes an `fs-open` request into a complete MSSP frame: body
/// `[argv]` with a `UniFsOpenArgv` record.
pub fn serialize_fs_open(argv: &UniFsOpenArgv) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_fs_open_request(argv))
}

/// Deserializes an `fs-open` result MSSP frame into the new file descriptor.
pub fn deserialize_fs_open_result(bytes: &[u8]) -> Result<UniReturn<u32>, ApiError> {
    let result = gen_codec::decode_fs_open_result(bytes).map_err(payload::frame_error_to_api)?;
    Ok(map_fs_errno(result))
}

// ---- fs-close ----

/// Serializes an `fs-close` request into a complete MSSP frame: body `[fd]`.
pub fn serialize_fs_close(fd: u32) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_fs_close_request(fd))
}

/// Deserializes an `fs-close` result MSSP frame.
pub fn deserialize_fs_close_result(bytes: &[u8]) -> Result<UniReturn<()>, ApiError> {
    let result = gen_codec::decode_fs_close_result(bytes).map_err(payload::frame_error_to_api)?;
    Ok(map_fs_errno(result))
}

// ---- fs-read ----

/// Serializes an `fs-read` request into a complete MSSP frame: body
/// `[fd, len]`.
pub fn serialize_fs_read(fd: u32, len: u32) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_fs_read_request(fd, len))
}

/// Deserializes an `fs-read` result MSSP frame into the read bytes.
pub fn deserialize_fs_read_result(bytes: &[u8]) -> Result<UniReturn<Vec<u8>>, ApiError> {
    let result = gen_codec::decode_fs_read_result(bytes).map_err(payload::frame_error_to_api)?;
    Ok(map_fs_errno(result))
}

// ---- fs-write ----

/// Serializes an `fs-write` request into a complete MSSP frame: body
/// `[fd, data]`.
pub fn serialize_fs_write(fd: u32, data: &[u8]) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_fs_write_request(fd, data))
}

/// Deserializes an `fs-write` result MSSP frame into the written byte count.
pub fn deserialize_fs_write_result(bytes: &[u8]) -> Result<UniReturn<u32>, ApiError> {
    let result = gen_codec::decode_fs_write_result(bytes).map_err(payload::frame_error_to_api)?;
    Ok(map_fs_errno(result))
}

// ---- fs-pread ----

/// Serializes an `fs-pread` request into a complete MSSP frame: body
/// `[fd, offset, len]`.
pub fn serialize_fs_pread(fd: u32, offset: u64, len: u32) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_fs_pread_request(fd, offset, len))
}

/// Deserializes an `fs-pread` result MSSP frame into the read bytes.
pub fn deserialize_fs_pread_result(bytes: &[u8]) -> Result<UniReturn<Vec<u8>>, ApiError> {
    let result = gen_codec::decode_fs_pread_result(bytes).map_err(payload::frame_error_to_api)?;
    Ok(map_fs_errno(result))
}

// ---- fs-pwrite ----

/// Serializes an `fs-pwrite` request into a complete MSSP frame: body
/// `[fd, offset, data]`.
pub fn serialize_fs_pwrite(fd: u32, offset: u64, data: &[u8]) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_fs_pwrite_request(fd, offset, data))
}

/// Deserializes an `fs-pwrite` result MSSP frame.
pub fn deserialize_fs_pwrite_result(bytes: &[u8]) -> Result<UniReturn<()>, ApiError> {
    let result = gen_codec::decode_fs_pwrite_result(bytes).map_err(payload::frame_error_to_api)?;
    Ok(map_fs_errno(result))
}

// ---- fs-lseek ----

/// Serializes an `fs-lseek` request into a complete MSSP frame: body
/// `[fd, offset, whence]`.
pub fn serialize_fs_lseek(fd: u32, offset: i64, whence: u32) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_fs_lseek_request(fd, offset, whence))
}

/// Deserializes an `fs-lseek` result MSSP frame into the new cursor position.
pub fn deserialize_fs_lseek_result(bytes: &[u8]) -> Result<UniReturn<u64>, ApiError> {
    let result = gen_codec::decode_fs_lseek_result(bytes).map_err(payload::frame_error_to_api)?;
    Ok(map_fs_errno(result))
}

// ---- fs-fstat ----

/// Serializes an `fs-fstat` request into a complete MSSP frame: body `[fd]`.
pub fn serialize_fs_fstat(fd: u32) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_fs_fstat_request(fd))
}

/// Deserializes an `fs-fstat` result MSSP frame into the stat record.
pub fn deserialize_fs_fstat_result(bytes: &[u8]) -> Result<UniReturn<UniFsStat>, ApiError> {
    let result = gen_codec::decode_fs_fstat_result(bytes).map_err(payload::frame_error_to_api)?;
    Ok(map_fs_errno(result))
}

// ---- fs-stat ----

/// Serializes an `fs-stat` request into a complete MSSP frame: body
/// `[oid, path]`.
pub fn serialize_fs_stat(oid: &UniOid, path: &str) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_fs_stat_request(oid, path))
}

/// Deserializes an `fs-stat` result MSSP frame into the stat record.
pub fn deserialize_fs_stat_result(bytes: &[u8]) -> Result<UniReturn<UniFsStat>, ApiError> {
    let result = gen_codec::decode_fs_stat_result(bytes).map_err(payload::frame_error_to_api)?;
    Ok(map_fs_errno(result))
}

// ---- fs-fsync ----

/// Serializes an `fs-fsync` request into a complete MSSP frame: body `[fd]`.
pub fn serialize_fs_fsync(fd: u32) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_fs_fsync_request(fd))
}

/// Deserializes an `fs-fsync` result MSSP frame.
pub fn deserialize_fs_fsync_result(bytes: &[u8]) -> Result<UniReturn<()>, ApiError> {
    let result = gen_codec::decode_fs_fsync_result(bytes).map_err(payload::frame_error_to_api)?;
    Ok(map_fs_errno(result))
}

// ---- fs-readdir ----

/// Serializes an `fs-readdir` request into a complete MSSP frame: body
/// `[oid, path]`.
pub fn serialize_fs_readdir(oid: &UniOid, path: &str) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_fs_readdir_request(oid, path))
}

/// Deserializes an `fs-readdir` result MSSP frame into the directory entries.
pub fn deserialize_fs_readdir_result(
    bytes: &[u8],
) -> Result<UniReturn<Vec<UniFsDirent>>, ApiError> {
    let result = gen_codec::decode_fs_readdir_result(bytes).map_err(payload::frame_error_to_api)?;
    Ok(map_fs_errno(result))
}

// ---- mock-side request decoders / response encoders ----

/// Decodes an `fs-open` request MSSP frame into its argument record.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn decode_fs_open_request(frame: &[u8]) -> Result<UniFsOpenArgv, ApiError> {
    gen_codec::decode_fs_open_request(frame).map_err(payload::frame_error_to_api)
}

/// Decodes an `fs-close` request MSSP frame into the file descriptor.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn decode_fs_close_request(frame: &[u8]) -> Result<u32, ApiError> {
    gen_codec::decode_fs_close_request(frame).map_err(payload::frame_error_to_api)
}

/// Decodes an `fs-read` request MSSP frame into `(fd, len)`.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn decode_fs_read_request(frame: &[u8]) -> Result<(u32, u32), ApiError> {
    gen_codec::decode_fs_read_request(frame).map_err(payload::frame_error_to_api)
}

/// Decodes an `fs-write` request MSSP frame into `(fd, data)`.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn decode_fs_write_request(frame: &[u8]) -> Result<(u32, Vec<u8>), ApiError> {
    gen_codec::decode_fs_write_request(frame).map_err(payload::frame_error_to_api)
}

/// Decodes an `fs-pread` request MSSP frame into `(fd, offset, len)`.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn decode_fs_pread_request(frame: &[u8]) -> Result<(u32, u64, u32), ApiError> {
    gen_codec::decode_fs_pread_request(frame).map_err(payload::frame_error_to_api)
}

/// Decodes an `fs-pwrite` request MSSP frame into `(fd, offset, data)`.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn decode_fs_pwrite_request(frame: &[u8]) -> Result<(u32, u64, Vec<u8>), ApiError> {
    gen_codec::decode_fs_pwrite_request(frame).map_err(payload::frame_error_to_api)
}

/// Decodes an `fs-lseek` request MSSP frame into `(fd, offset, whence)`.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn decode_fs_lseek_request(frame: &[u8]) -> Result<(u32, i64, u32), ApiError> {
    gen_codec::decode_fs_lseek_request(frame).map_err(payload::frame_error_to_api)
}

/// Decodes an `fs-fstat` request MSSP frame into the file descriptor.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn decode_fs_fstat_request(frame: &[u8]) -> Result<u32, ApiError> {
    gen_codec::decode_fs_fstat_request(frame).map_err(payload::frame_error_to_api)
}

/// Decodes an `fs-stat` request MSSP frame into `(oid, path)`.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn decode_fs_stat_request(frame: &[u8]) -> Result<(UniOid, String), ApiError> {
    gen_codec::decode_fs_stat_request(frame).map_err(payload::frame_error_to_api)
}

/// Decodes an `fs-fsync` request MSSP frame into the file descriptor.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn decode_fs_fsync_request(frame: &[u8]) -> Result<u32, ApiError> {
    gen_codec::decode_fs_fsync_request(frame).map_err(payload::frame_error_to_api)
}

/// Decodes an `fs-readdir` request MSSP frame into `(oid, path)`.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn decode_fs_readdir_request(frame: &[u8]) -> Result<(UniOid, String), ApiError> {
    gen_codec::decode_fs_readdir_request(frame).map_err(payload::frame_error_to_api)
}

/// Encodes an `fs-open` result (or error) into a complete MSSP frame.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn encode_fs_open_response(result: &UniReturn<u32>) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_fs_open_result(result))
}

/// Encodes an `fs-close` result (or error) into a complete MSSP frame.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn encode_fs_close_response(result: &UniReturn<()>) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_fs_close_result(result))
}

/// Encodes an `fs-read` result (or error) into a complete MSSP frame: the
/// value arm is a bin.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn encode_fs_read_response(result: &UniReturn<Vec<u8>>) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_fs_read_result(result))
}

/// Encodes an `fs-write` result (or error) into a complete MSSP frame.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn encode_fs_write_response(result: &UniReturn<u32>) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_fs_write_result(result))
}

/// Encodes an `fs-pread` result (or error) into a complete MSSP frame: the
/// value arm is a bin.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn encode_fs_pread_response(result: &UniReturn<Vec<u8>>) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_fs_pread_result(result))
}

/// Encodes an `fs-pwrite` result (or error) into a complete MSSP frame.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn encode_fs_pwrite_response(result: &UniReturn<()>) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_fs_pwrite_result(result))
}

/// Encodes an `fs-lseek` result (or error) into a complete MSSP frame.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn encode_fs_lseek_response(result: &UniReturn<u64>) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_fs_lseek_result(result))
}

/// Encodes an `fs-fstat` result (or error) into a complete MSSP frame.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn encode_fs_fstat_response(result: &UniReturn<UniFsStat>) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_fs_fstat_result(result))
}

/// Encodes an `fs-stat` result (or error) into a complete MSSP frame.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn encode_fs_stat_response(result: &UniReturn<UniFsStat>) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_fs_stat_result(result))
}

/// Encodes an `fs-fsync` result (or error) into a complete MSSP frame.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn encode_fs_fsync_response(result: &UniReturn<()>) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_fs_fsync_result(result))
}

/// Encodes an `fs-readdir` result (or error) into a complete MSSP frame.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn encode_fs_readdir_response(
    result: &UniReturn<Vec<UniFsDirent>>,
) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_fs_readdir_result(result))
}

// ---- typed syscall wrappers ----

/// Runs an `fs-open` syscall and returns the new file descriptor.
pub async fn sys_fs_open(argv: &UniFsOpenArgv) -> Result<UniReturn<u32>, ApiError> {
    let request = serialize_fs_open(argv)?;
    let response = fs_open_raw(request).await?;
    deserialize_fs_open_result(&response)
}

/// Runs an `fs-close` syscall.
pub async fn sys_fs_close(fd: u32) -> Result<UniReturn<()>, ApiError> {
    let request = serialize_fs_close(fd)?;
    let response = fs_close_raw(request).await?;
    deserialize_fs_close_result(&response)
}

/// Runs an `fs-read` syscall and returns the read bytes.
pub async fn sys_fs_read(fd: u32, len: u32) -> Result<UniReturn<Vec<u8>>, ApiError> {
    let request = serialize_fs_read(fd, len)?;
    let response = fs_read_raw(request).await?;
    deserialize_fs_read_result(&response)
}

/// Runs an `fs-write` syscall and returns the written byte count.
pub async fn sys_fs_write(fd: u32, data: &[u8]) -> Result<UniReturn<u32>, ApiError> {
    let request = serialize_fs_write(fd, data)?;
    let response = fs_write_raw(request).await?;
    deserialize_fs_write_result(&response)
}

/// Runs an `fs-pread` syscall and returns the read bytes.
pub async fn sys_fs_pread(fd: u32, offset: u64, len: u32) -> Result<UniReturn<Vec<u8>>, ApiError> {
    let request = serialize_fs_pread(fd, offset, len)?;
    let response = fs_pread_raw(request).await?;
    deserialize_fs_pread_result(&response)
}

/// Runs an `fs-pwrite` syscall.
pub async fn sys_fs_pwrite(fd: u32, offset: u64, data: &[u8]) -> Result<UniReturn<()>, ApiError> {
    let request = serialize_fs_pwrite(fd, offset, data)?;
    let response = fs_pwrite_raw(request).await?;
    deserialize_fs_pwrite_result(&response)
}

/// Runs an `fs-lseek` syscall and returns the new cursor position.
pub async fn sys_fs_lseek(fd: u32, offset: i64, whence: u32) -> Result<UniReturn<u64>, ApiError> {
    let request = serialize_fs_lseek(fd, offset, whence)?;
    let response = fs_lseek_raw(request).await?;
    deserialize_fs_lseek_result(&response)
}

/// Runs an `fs-fstat` syscall and returns the stat record.
pub async fn sys_fs_fstat(fd: u32) -> Result<UniReturn<UniFsStat>, ApiError> {
    let request = serialize_fs_fstat(fd)?;
    let response = fs_fstat_raw(request).await?;
    deserialize_fs_fstat_result(&response)
}

/// Runs an `fs-stat` syscall and returns the stat record.
pub async fn sys_fs_stat(oid: &UniOid, path: &str) -> Result<UniReturn<UniFsStat>, ApiError> {
    let request = serialize_fs_stat(oid, path)?;
    let response = fs_stat_raw(request).await?;
    deserialize_fs_stat_result(&response)
}

/// Runs an `fs-fsync` syscall.
pub async fn sys_fs_fsync(fd: u32) -> Result<UniReturn<()>, ApiError> {
    let request = serialize_fs_fsync(fd)?;
    let response = fs_fsync_raw(request).await?;
    deserialize_fs_fsync_result(&response)
}

/// Runs an `fs-readdir` syscall and returns the directory entries.
pub async fn sys_fs_readdir(
    oid: &UniOid,
    path: &str,
) -> Result<UniReturn<Vec<UniFsDirent>>, ApiError> {
    let request = serialize_fs_readdir(oid, path)?;
    let response = fs_readdir_raw(request).await?;
    deserialize_fs_readdir_result(&response)
}

macro_rules! fs_raw {
    ($name:ident, $param:ident, $mock:ident, $wasm:ident) => {
        #[allow(unused_variables)]
        pub async fn $name($param: Vec<u8>) -> Result<Vec<u8>, ApiError> {
            #[cfg(feature = "mock-sqlite")]
            {
                return crate::mock::MockSqliteMuduSysCall::$mock($param).await;
            }

            #[cfg(all(
                target_arch = "wasm32",
                feature = "wasm-async",
                not(feature = "mock-sqlite")
            ))]
            {
                return super::wasm_async::$wasm($param).await;
            }

            #[allow(unreachable_code)]
            Err(ApiError::backend_unavailable(
                "no mudu backend configured; enable `mock-sqlite` or build wasm32 with `wasm-async`",
            ))
        }
    };
}

fs_raw!(fs_open_raw, fs_open_in, fs_open_raw, fs_open_raw);
fs_raw!(fs_close_raw, fs_close_in, fs_close_raw, fs_close_raw);
fs_raw!(fs_read_raw, fs_read_in, fs_read_raw, fs_read_raw);
fs_raw!(fs_write_raw, fs_write_in, fs_write_raw, fs_write_raw);
fs_raw!(fs_pread_raw, fs_pread_in, fs_pread_raw, fs_pread_raw);
fs_raw!(fs_pwrite_raw, fs_pwrite_in, fs_pwrite_raw, fs_pwrite_raw);
fs_raw!(fs_lseek_raw, fs_lseek_in, fs_lseek_raw, fs_lseek_raw);
fs_raw!(fs_fstat_raw, fs_fstat_in, fs_fstat_raw, fs_fstat_raw);
fs_raw!(fs_stat_raw, fs_stat_in, fs_stat_raw, fs_stat_raw);
fs_raw!(fs_fsync_raw, fs_fsync_in, fs_fsync_raw, fs_fsync_raw);
fs_raw!(
    fs_readdir_raw,
    fs_readdir_in,
    fs_readdir_raw,
    fs_readdir_raw
);

#[cfg(test)]
mod tests {
    use super::*;

    fn oid(l: u64) -> UniOid {
        UniOid { h: 0, l }
    }

    fn fs_open_argv() -> UniFsOpenArgv {
        UniFsOpenArgv {
            session: oid(11),
            oid: oid(22),
            path: "docs/a.txt".to_string(),
            flags: 2,
        }
    }

    fn stat() -> UniFsStat {
        UniFsStat {
            oid: oid(5),
            generation: 1,
            entry: String::new(),
            length: 100,
            state: 1,
        }
    }

    #[test]
    fn fs_open_request_frame_matches_golden_bytes() {
        let frame = serialize_fs_open(&fs_open_argv()).unwrap();
        let expected: &[u8] = &[
            0x4D, 0x53, 0x53, 0x50, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x0A, // kind FsOpen
            0x81, 0x01, // {1: argv}
            0x84, // argv: a 4-field record map
            0x01, 0x82, 0x01, 0x00, 0x02, 0x0B, // session {1: 0, 2: 11}
            0x02, 0x82, 0x01, 0x00, 0x02, 0x16, // oid {1: 0, 2: 22}
            0x03, 0xAA, 0x64, 0x6F, 0x63, 0x73, 0x2F, 0x61, 0x2E, 0x74, 0x78, 0x74, // path
            0x04, 0x02, // flags
        ];
        assert_eq!(frame, expected);

        let decoded = decode_fs_open_request(&frame).unwrap();
        assert_eq!(decoded.session.l, 11);
        assert_eq!(decoded.oid.l, 22);
        assert_eq!(decoded.path, "docs/a.txt");
        assert_eq!(decoded.flags, 2);
    }

    #[test]
    fn fd_request_frames_match_golden_bytes() {
        // fs-close / fs-fstat / fs-fsync share the `[fd]` request body.
        let frame = serialize_fs_close(3).unwrap();
        assert_eq!(
            &frame[..16],
            &[
                0x4D, 0x53, 0x53, 0x50, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x0B,
            ]
        );
        assert_eq!(&frame[16..], &[0x81, 0x01, 0x03]);
        assert_eq!(decode_fs_close_request(&frame).unwrap(), 3);

        let frame = serialize_fs_fstat(3).unwrap();
        assert_eq!(frame[15], 0x11); // kind FsFstat
        assert_eq!(&frame[16..], &[0x81, 0x01, 0x03]);
        assert_eq!(decode_fs_fstat_request(&frame).unwrap(), 3);

        let frame = serialize_fs_fsync(3).unwrap();
        assert_eq!(frame[15], 0x13); // kind FsFsync
        assert_eq!(&frame[16..], &[0x81, 0x01, 0x03]);
        assert_eq!(decode_fs_fsync_request(&frame).unwrap(), 3);
    }

    #[test]
    fn fs_read_request_frame_matches_golden_bytes() {
        let frame = serialize_fs_read(3, 4).unwrap();
        assert_eq!(frame[15], 0x0C); // kind FsRead
        assert_eq!(&frame[16..], &[0x82, 0x01, 0x03, 0x02, 0x04]);
        assert_eq!(decode_fs_read_request(&frame).unwrap(), (3, 4));
    }

    #[test]
    fn fs_write_request_frame_matches_golden_bytes() {
        let frame = serialize_fs_write(3, b"hi").unwrap();
        assert_eq!(frame[15], 0x0D); // kind FsWrite
        assert_eq!(
            &frame[16..],
            &[0x82, 0x01, 0x03, 0x02, 0xC4, 0x02, 0x68, 0x69]
        );
        assert_eq!(
            decode_fs_write_request(&frame).unwrap(),
            (3, b"hi".to_vec())
        );
    }

    #[test]
    fn fs_pread_request_frame_matches_golden_bytes() {
        let frame = serialize_fs_pread(3, 8, 4).unwrap();
        assert_eq!(frame[15], 0x0E); // kind FsPread
        assert_eq!(&frame[16..], &[0x83, 0x01, 0x03, 0x02, 0x08, 0x03, 0x04]);
        assert_eq!(decode_fs_pread_request(&frame).unwrap(), (3, 8, 4));
    }

    #[test]
    fn fs_pwrite_request_frame_matches_golden_bytes() {
        let frame = serialize_fs_pwrite(3, 8, b"hi").unwrap();
        assert_eq!(frame[15], 0x0F); // kind FsPwrite
        assert_eq!(
            &frame[16..],
            &[0x83, 0x01, 0x03, 0x02, 0x08, 0x03, 0xC4, 0x02, 0x68, 0x69]
        );
        assert_eq!(
            decode_fs_pwrite_request(&frame).unwrap(),
            (3, 8, b"hi".to_vec())
        );
    }

    #[test]
    fn fs_lseek_request_frame_matches_golden_bytes() {
        let frame = serialize_fs_lseek(3, -2, 1).unwrap();
        assert_eq!(frame[15], 0x10); // kind FsLseek
        assert_eq!(&frame[16..], &[0x83, 0x01, 0x03, 0x02, 0xFE, 0x03, 0x01]);
        assert_eq!(decode_fs_lseek_request(&frame).unwrap(), (3, -2, 1));
    }

    #[test]
    fn fs_stat_request_frame_matches_golden_bytes() {
        let frame = serialize_fs_stat(&oid(22), "a").unwrap();
        assert_eq!(frame[15], 0x12); // kind FsStat
        assert_eq!(
            &frame[16..],
            &[0x82, 0x01, 0x82, 0x01, 0x00, 0x02, 0x16, 0x02, 0xA1, 0x61]
        );
        let (decoded_oid, path) = decode_fs_stat_request(&frame).unwrap();
        assert_eq!(decoded_oid.l, 22);
        assert_eq!(path, "a");
    }

    #[test]
    fn fs_readdir_request_frame_matches_golden_bytes() {
        let frame = serialize_fs_readdir(&oid(23), "d").unwrap();
        assert_eq!(frame[15], 0x14); // kind FsReaddir
        assert_eq!(
            &frame[16..],
            &[0x82, 0x01, 0x82, 0x01, 0x00, 0x02, 0x17, 0x02, 0xA1, 0x64]
        );
        let (decoded_oid, path) = decode_fs_readdir_request(&frame).unwrap();
        assert_eq!(decoded_oid.l, 23);
        assert_eq!(path, "d");
    }

    #[test]
    fn fs_open_result_frame_roundtrips_fd_and_err() {
        let frame = encode_fs_open_response(&Ok(9)).unwrap();
        assert_eq!(frame[15], 0x0A);
        assert_eq!(&frame[16..], &[0x92, 0x00, 0x09]);
        match deserialize_fs_open_result(&frame).unwrap() {
            Ok(fd) => assert_eq!(fd, 9),
            Err(error) => panic!("unexpected error: {}", error.err_msg),
        }

        let frame = encode_fs_open_response(&Err(UniError {
            err_code: 2,
            err_msg: "no such entry".to_string(),
            ..Default::default()
        }))
        .unwrap();
        match deserialize_fs_open_result(&frame).unwrap() {
            Ok(_) => panic!("expected error variant"),
            Err(error) => assert_eq!(error.err_code, 2),
        }
    }

    #[test]
    fn fs_read_result_frame_roundtrips_bin_and_err() {
        let frame = encode_fs_read_response(&Ok(b"hi".to_vec())).unwrap();
        assert_eq!(frame[15], 0x0C);
        assert_eq!(&frame[16..], &[0x92, 0x00, 0xC4, 0x02, 0x68, 0x69]);
        match deserialize_fs_read_result(&frame).unwrap() {
            Ok(data) => assert_eq!(data, b"hi"),
            Err(error) => panic!("unexpected error: {}", error.err_msg),
        }

        let frame = encode_fs_pread_response(&Ok(b"hi".to_vec())).unwrap();
        assert_eq!(frame[15], 0x0E);
        assert_eq!(&frame[16..], &[0x92, 0x00, 0xC4, 0x02, 0x68, 0x69]);
        match deserialize_fs_pread_result(&frame).unwrap() {
            Ok(data) => assert_eq!(data, b"hi"),
            Err(error) => panic!("unexpected error: {}", error.err_msg),
        }

        // Trailing bytes after the body are rejected.
        let mut trailing = encode_fs_read_response(&Ok(b"hi".to_vec())).unwrap();
        trailing.push(0x00);
        assert!(deserialize_fs_read_result(&trailing).is_err());
    }

    #[test]
    fn fs_write_result_frame_roundtrips_count() {
        let frame = encode_fs_write_response(&Ok(2)).unwrap();
        assert_eq!(frame[15], 0x0D);
        assert_eq!(&frame[16..], &[0x92, 0x00, 0x02]);
        match deserialize_fs_write_result(&frame).unwrap() {
            Ok(written) => assert_eq!(written, 2),
            Err(error) => panic!("unexpected error: {}", error.err_msg),
        }
    }

    #[test]
    fn fs_lseek_result_frame_roundtrips_position() {
        let frame = encode_fs_lseek_response(&Ok(6)).unwrap();
        assert_eq!(frame[15], 0x10);
        assert_eq!(&frame[16..], &[0x92, 0x00, 0x06]);
        match deserialize_fs_lseek_result(&frame).unwrap() {
            Ok(position) => assert_eq!(position, 6),
            Err(error) => panic!("unexpected error: {}", error.err_msg),
        }
    }

    #[test]
    fn fs_stat_result_frame_roundtrips_record() {
        let frame = encode_fs_fstat_response(&Ok(stat())).unwrap();
        assert_eq!(frame[15], 0x11);
        let expected_body: &[u8] = &[
            0x92, 0x00, // [0, stat]
            0x85, // stat: a 5-field record map
            0x01, 0x82, 0x01, 0x00, 0x02, 0x05, // oid {1: 0, 2: 5}
            0x02, 0x01, // generation
            0x03, 0xA0, // entry ""
            0x04, 0x64, // length 100
            0x05, 0x01, // state
        ];
        assert_eq!(&frame[16..], expected_body);
        match deserialize_fs_fstat_result(&frame).unwrap() {
            Ok(decoded) => {
                assert_eq!(decoded.oid.l, 5);
                assert_eq!(decoded.generation, 1);
                assert_eq!(decoded.length, 100);
                assert_eq!(decoded.state, 1);
            }
            Err(error) => panic!("unexpected error: {}", error.err_msg),
        }

        let frame = encode_fs_stat_response(&Ok(stat())).unwrap();
        assert_eq!(frame[15], 0x12);
        assert_eq!(&frame[16..], expected_body);
        assert!(deserialize_fs_stat_result(&frame).unwrap().is_ok());
    }

    #[test]
    fn fs_unit_result_frames_roundtrip() {
        for (encode, deserialize, kind) in [
            (
                encode_fs_close_response as fn(&UniReturn<()>) -> Result<Vec<u8>, ApiError>,
                deserialize_fs_close_result as fn(&[u8]) -> Result<UniReturn<()>, ApiError>,
                0x0Bu8,
            ),
            (
                encode_fs_pwrite_response,
                deserialize_fs_pwrite_result,
                0x0F,
            ),
            (encode_fs_fsync_response, deserialize_fs_fsync_result, 0x13),
        ] {
            let frame = encode(&Ok(())).unwrap();
            assert_eq!(frame[15], kind);
            assert_eq!(&frame[16..], &[0x92, 0x00, 0x00]);
            assert!(deserialize(&frame).unwrap().is_ok());
        }
    }

    #[test]
    fn fs_readdir_result_frame_roundtrips_entries() {
        let entries = vec![
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
        ];
        let frame = encode_fs_readdir_response(&Ok(entries)).unwrap();
        assert_eq!(frame[15], 0x14);
        let expected_body: &[u8] = &[
            0x92, 0x00, 0x92, // [0, 2 entries]
            0x83, 0x01, 0xA5, 0x61, 0x2E, 0x74, 0x78, 0x74, 0x02, 0xC2, 0x03,
            0x03, // {"a.txt", false, 3}
            0x83, 0x01, 0xA4, 0x64, 0x6F, 0x63, 0x73, 0x02, 0xC3, 0x03,
            0x00, // {"docs", true, 0}
        ];
        assert_eq!(&frame[16..], expected_body);
        match deserialize_fs_readdir_result(&frame).unwrap() {
            Ok(decoded) => {
                assert_eq!(decoded.len(), 2);
                assert_eq!(decoded[0].name, "a.txt");
                assert!(!decoded[0].is_dir);
                assert_eq!(decoded[0].length, 3);
                assert!(decoded[1].is_dir);
            }
            Err(error) => panic!("unexpected error: {}", error.err_msg),
        }
    }

    #[test]
    fn invalid_argument_maps_to_einval_on_fs_results() {
        let frame = encode_fs_open_response(&Err(UniError {
            err_code: FS_ERR_INVALID_ARGUMENT,
            err_msg: "unsupported fs open flags 0o101".to_string(),
            ..Default::default()
        }))
        .unwrap();
        match deserialize_fs_open_result(&frame).unwrap() {
            Ok(_) => panic!("expected error variant"),
            Err(error) => {
                assert_eq!(error.err_code, FS_ERR_INVALID_INPUT);
                assert!(error.err_msg.contains("unsupported fs open flags"));
            }
        }
    }

    #[test]
    fn other_error_codes_pass_through_unchanged() {
        let frame = encode_fs_read_response(&Err(UniError {
            err_code: 9, // EBADF
            err_msg: "fs fd 3 is not open for reading".to_string(),
            ..Default::default()
        }))
        .unwrap();
        match deserialize_fs_read_result(&frame).unwrap() {
            Ok(_) => panic!("expected error variant"),
            Err(error) => assert_eq!(error.err_code, 9),
        }
    }
}
