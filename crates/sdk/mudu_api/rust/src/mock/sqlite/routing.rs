//! MSSP frame routing continuation block: validates the frame header and
//! dispatches each message kind to the per-kind handlers grouped in the
//! sibling modules (`sql`, `session_kv`, `fs_route`).

use super::MockSqliteMuduSysCall;
use crate::error::ApiError;
use crate::mudu_sys::payload;

impl MockSqliteMuduSysCall {
    /// Validates the MSSP header and routes by message kind: query/command/
    /// batch go to the SQLite emulation, session/KV/relation kinds to the
    /// in-memory KV emulation and the fs kinds to the in-memory fs emulation.
    fn route_frame(frame: &[u8]) -> Result<Vec<u8>, ApiError> {
        let (kind, _) = payload::decode_any_frame(frame)?;
        match kind {
            payload::KIND_BATCH => Self::route_batch(frame),
            payload::KIND_OPEN => Self::route_open(frame),
            payload::KIND_CLOSE => Self::route_close(frame),
            payload::KIND_GET => Self::route_get(frame),
            payload::KIND_PUT => Self::route_put(frame),
            payload::KIND_DELETE => Self::route_delete(frame),
            payload::KIND_RANGE => Self::route_range(frame),
            payload::KIND_RELATION_GET => Self::route_relation_get(frame),
            payload::KIND_RELATION_UPDATE => Self::route_relation_update(frame),
            payload::KIND_RELATION_INSERT => Self::route_relation_insert(frame),
            payload::KIND_FS_OPEN => Self::route_fs_open(frame),
            payload::KIND_FS_CLOSE => Self::route_fs_close(frame),
            payload::KIND_FS_READ => Self::route_fs_read(frame),
            payload::KIND_FS_WRITE => Self::route_fs_write(frame),
            payload::KIND_FS_PREAD => Self::route_fs_pread(frame),
            payload::KIND_FS_PWRITE => Self::route_fs_pwrite(frame),
            payload::KIND_FS_LSEEK => Self::route_fs_lseek(frame),
            payload::KIND_FS_FSTAT => Self::route_fs_fstat(frame),
            payload::KIND_FS_STAT => Self::route_fs_stat(frame),
            payload::KIND_FS_FSYNC => Self::route_fs_fsync(frame),
            payload::KIND_FS_READDIR => Self::route_fs_readdir(frame),
            other => Err(ApiError::Decode(format!(
                "mock does not emulate syscall message kind {other}"
            ))),
        }
    }

    /// Runs `route_frame` on the blocking pool for one raw frame handler.
    pub(super) async fn routed(frame: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        tokio::task::spawn_blocking(move || Self::route_frame(&frame)).await?
    }
}
