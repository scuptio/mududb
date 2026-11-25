use crate::page::page_block_ref::PAGE_SIZE;
use crate::page::PageId;
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

pub(super) async fn read_file_exact(file: &SysFile, len: usize, offset: u64) -> RS<Vec<u8>> {
    file.read_exact_at(offset, len).await
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
    let file = default_sys_io_context()
        .fs()
        .open(
            path,
            FileOptions::new(
                libc::O_CREAT | libc::O_RDWR | libc::O_CLOEXEC,
                super::FILE_MODE_644,
            ),
        )
        .await?;
    file.close().await
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn page_offset_rejects_overflowing_page_ids() {
        // u64::MAX * PAGE_SIZE overflows: an IndexOutOfRange error, not a wrap.
        let err = page_offset(PageId::INVALID).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::IndexOutOfRange);
        assert_eq!(page_offset(PageId::new(2)).unwrap(), 2 * PAGE_SIZE as u64);
    }

    #[test]
    fn ensure_and_remove_file_async_paths() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let dir = mudu_sys::env_var::temp_dir()
                .join(format!("ts-io-ensure-{}", mudu_sys::random::uuid_v4()));
            let path = dir.join("nested").join("f.dat");

            // create_dir_all + open-create through the default provider.
            ensure_time_series_file_exists_async(default_sys_io_context().fs().as_ref(), &path)
                .await
                .unwrap();
            assert!(mudu_sys::io::path::path_exists(&path).await.unwrap());

            // Idempotent: the second call takes the exists branch.
            ensure_time_series_file_exists_async_no_fs(&path)
                .await
                .unwrap();

            // Positional writes and reads through the opened file.
            let file = default_sys_io_context()
                .fs()
                .open(
                    &path,
                    FileOptions::new(libc::O_RDWR | libc::O_CLOEXEC, 0o644),
                )
                .await
                .unwrap();
            file.write_all_at(0, b"abcd").await.unwrap();
            file.fsync().await.unwrap();
            let sys_file = SysFile::new(file);
            assert_eq!(read_file_exact(&sys_file, 4, 0).await.unwrap(), b"abcd");

            remove_file_if_exists_async(&path).await.unwrap();
            assert!(!mudu_sys::io::path::path_exists(&path).await.unwrap());
            // Removing a missing file is a no-op.
            remove_file_if_exists_async(&path).await.unwrap();
            let _ = mudu_sys::fs::sync::remove_dir_all(&dir);
        })
        .unwrap();
    }
}
