use crate::contract::async_file::AsyncFile;
use crate::contract::async_fs::AsyncFs;
use crate::contract::file_options::FileOptions;
use crate::imp::fs::async_io_uring;
use crate::imp::native::fs::async_io_uring::async_io_uring_file::AsyncIoUringFile;
use crate::scoped_task_trace;
use async_trait::async_trait;
use mudu::common::result::RS;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Default)]
pub struct AsyncIoUringFs;

impl AsyncIoUringFs {
    pub(crate) fn new() -> Self {
        Self
    }
}
#[async_trait]
impl AsyncFs for AsyncIoUringFs {
    async fn open(&self, path: &Path, options: FileOptions) -> RS<Arc<dyn AsyncFile>> {
        Ok(Arc::new(AsyncIoUringFile::open(path, options).await?))
    }

    async fn create_dir_all(&self, path: &Path) -> RS<()> {
        scoped_task_trace!();
        async_io_uring::create_dir_all(path).await
    }

    async fn metadata_len(&self, path: &Path) -> RS<u64> {
        async_io_uring::metadata_len(path).await
    }

    async fn path_exists(&self, path: &Path) -> RS<bool> {
        async_io_uring::path_exists(path).await
    }

    async fn remove_file_if_exists(&self, path: &Path) -> RS<()> {
        async_io_uring::remove_file_if_exists(path).await
    }

    async fn read_dir(&self, path: &Path) -> RS<Vec<PathBuf>> {
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(path).map_err(|e| {
            mudu::mudu_error!(mudu::error::ErrorCode::from(&e), "read directory error", e)
        })? {
            let entry = entry.map_err(|e| {
                mudu::mudu_error!(
                    mudu::error::ErrorCode::from(&e),
                    "read directory entry error",
                    e
                )
            })?;
            paths.push(entry.path());
        }
        Ok(paths)
    }

    async fn remove_dir_all(&self, path: &Path) -> RS<()> {
        // io_uring has no recursive directory removal op; mirror `read_dir`
        // above and perform the removal synchronously.
        std::fs::remove_dir_all(path).map_err(|e| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::from(&e),
                "remove directory tree error",
                e
            )
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::AsyncIoUringFs;
    use crate::contract::async_fs::AsyncFs;

    // `read_dir`/`remove_dir_all` mirror std::fs synchronously: no ring needed.
    #[tokio::test(flavor = "current_thread")]
    async fn read_dir_and_remove_dir_all_without_a_ring() {
        let root = std::env::temp_dir().join(format!("mudu-iou-fs-{}", crate::random::uuid_v4()));
        let nested = root.join("sub");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(root.join("a.txt"), b"a").unwrap();

        let fs = AsyncIoUringFs::new();
        let mut entries = fs.read_dir(&root).await.unwrap();
        entries.sort();
        assert!(entries.iter().any(|p| p.ends_with("a.txt")));
        assert!(entries.iter().any(|p| p.ends_with("sub")));
        assert!(fs.read_dir(&root.join("missing")).await.is_err());

        fs.remove_dir_all(&nested).await.unwrap();
        assert!(!nested.exists());
        fs.remove_dir_all(&root).await.unwrap();
        assert!(!root.exists());
        assert!(fs.remove_dir_all(&root).await.is_err());
    }
}
