use super::FILE_MODE_644;
use crate::storage::page::page_block_ref::PAGE_SIZE;
use crate::storage::page::PageId;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::contract::async_fs::AsyncFs;
use mudu_sys::contract::file_options::FileOptions;
use mudu_sys::default_sys_io_context;
use mudu_sys::fs::async_ as fs;
use mudu_sys::fs::SysFile;
use std::path::Path;
use tracing::trace;

pub(super) fn page_offset(page_id: PageId) -> RS<u64> {
    page_id
        .checked_mul(PAGE_SIZE as u64)
        .map(|offset| offset.as_u64())
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::IndexOutOfRange,
                "time series page offset overflow"
            )
        })
}

pub(super) async fn open_rw(fs: &dyn AsyncFs, path: &Path, flags: i32) -> RS<SysFile> {
    Ok(SysFile::new(
        fs.open(path, FileOptions::new(flags, FILE_MODE_644))
            .await?,
    ))
}

pub(super) async fn read_file_exact(file: &SysFile, len: usize, offset: u64) -> RS<Vec<u8>> {
    file.read_exact_at(offset, len).await
}

pub(super) async fn flush_file(file: &SysFile) -> RS<()> {
    file.fsync().await
}

/// Blocking `write_all_at` for the sync close path (`close_sync`), which
/// cannot await the async `SysFile` API. Writes through `libc::pwrite` on
/// the raw descriptor; on the native backend `SysFile` always wraps a real
/// fd. This is the only sanctioned blocking write in this module: async
/// callers must go through `SysFile::write_all_at`.
#[cfg(unix)]
pub(super) fn sync_write_all_at(file: &SysFile, offset: u64, payload: &[u8]) -> RS<()> {
    let fd = file.as_raw_fd().ok_or_else(|| {
        mudu_error!(
            ErrorCode::Internal,
            "sync page write requires a raw file descriptor"
        )
    })?;
    let mut written = 0usize;
    while written < payload.len() {
        let rc = unsafe {
            libc::pwrite(
                fd,
                payload[written..].as_ptr() as *const libc::c_void,
                payload.len() - written,
                (offset + written as u64) as libc::off_t,
            )
        };
        if rc < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(mudu_error!(
                ErrorCode::Internal,
                "sync page write error",
                err
            ));
        }
        if rc == 0 {
            return Err(mudu_error!(
                ErrorCode::Internal,
                "sync page write wrote zero bytes"
            ));
        }
        written += rc as usize;
    }
    Ok(())
}

#[cfg(not(unix))]
pub(super) fn sync_write_all_at(_file: &SysFile, _offset: u64, _payload: &[u8]) -> RS<()> {
    Err(mudu_error!(
        ErrorCode::NotImplemented,
        "sync page write is only supported on unix"
    ))
}

pub(super) async fn close_file(file: SysFile) -> RS<()> {
    file.close().await
}

pub(super) async fn remove_file_if_exists_async(path: &Path) -> RS<()> {
    fs::remove_file_if_exists(path).await
}

pub(super) async fn ensure_time_series_file_exists_async(fs: &dyn AsyncFs, path: &Path) -> RS<()> {
    if let Some(parent) = path.parent() {
        trace!(path = %path.display(), parent = %parent.display(), "time_series ensure file create_dir_all");
        fs.create_dir_all(parent).await?;
    }
    trace!(path = %path.display(), "time_series ensure file open create");
    let _ = fs.open(path, FileOptions::read_write_create()).await?;
    trace!(path = %path.display(), "time_series ensure file open create done");
    Ok(())
}

pub(super) async fn ensure_time_series_file_exists_async_no_fs(path: &Path) -> RS<()> {
    if let Some(parent) = path.parent() {
        mudu_sys::fs::async_::create_dir_all(parent).await?;
    }
    if mudu_sys::io::path::path_exists(path).await? {
        return Ok(());
    }
    let file = open_rw(
        default_sys_io_context().fs().as_ref(),
        path,
        libc::O_CREAT | libc::O_RDWR | libc::O_CLOEXEC,
    )
    .await?;
    close_file(file).await
}
