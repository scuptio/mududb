use crate::contract::async_file::AsyncFile;
use crate::contract::file_options::FileOptions;
use async_trait::async_trait;
use mudu::common::result::RS;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Abstraction over an async file system.
#[async_trait]
pub trait AsyncFs: Send + Sync {
    /// Open a file at `path` with the given options.
    async fn open(&self, path: &Path, options: FileOptions) -> RS<Arc<dyn AsyncFile>>;
    /// Create a directory and all of its parents.
    async fn create_dir_all(&self, path: &Path) -> RS<()>;
    /// Return the length of the file at `path`.
    async fn metadata_len(&self, path: &Path) -> RS<u64>;
    /// Return whether a file or directory exists at `path`.
    async fn path_exists(&self, path: &Path) -> RS<bool>;
    /// Remove the file at `path` if it exists.
    async fn remove_file_if_exists(&self, path: &Path) -> RS<()>;
    /// Read the entries of the directory at `path`.
    async fn read_dir(&self, path: &Path) -> RS<Vec<PathBuf>>;

    /// Read the entire contents of the file at `path`.
    async fn read_all(&self, path: &Path) -> RS<Vec<u8>> {
        let file = self.open(path, FileOptions::read_only()).await?;
        let len = file.file_len().await?;
        file.read_exact_at(0, len as usize).await
    }

    /// Remove a directory and all of its contents.
    async fn remove_dir_all(&self, _path: &Path) -> RS<()> {
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::NotImplemented,
            "remove_dir_all is not implemented"
        ))
    }

    /// Write `data` to the file at `path`, replacing any previous contents.
    async fn write_all(&self, _path: &Path, _data: &[u8]) -> RS<()> {
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::NotImplemented,
            "write_all is not implemented"
        ))
    }

    /// Read the entire contents of the file at `path` as a UTF-8 string.
    async fn read_to_string(&self, path: &Path) -> RS<String> {
        let bytes = self.read_all(path).await?;
        String::from_utf8(bytes)
            .map_err(|e| mudu::mudu_error!(mudu::error::ErrorCode::InvalidUtf8, "invalid utf8", e))
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use super::*;
    use mudu::error::ErrorCode;

    struct MockFile;

    #[async_trait]
    impl AsyncFile for MockFile {
        async fn read_exact_at(&self, _offset: u64, _len: usize) -> RS<Vec<u8>> {
            Ok(vec![])
        }

        async fn write_all_at(&self, _offset: u64, _payload: &[u8]) -> RS<()> {
            Ok(())
        }

        async fn fsync(&self) -> RS<()> {
            Ok(())
        }

        async fn file_len(&self) -> RS<u64> {
            Ok(0)
        }
    }

    struct MockFs;

    #[async_trait]
    impl AsyncFs for MockFs {
        async fn open(&self, _path: &Path, _options: FileOptions) -> RS<Arc<dyn AsyncFile>> {
            Ok(Arc::new(MockFile))
        }

        async fn create_dir_all(&self, _path: &Path) -> RS<()> {
            Ok(())
        }

        async fn metadata_len(&self, _path: &Path) -> RS<u64> {
            Ok(0)
        }

        async fn path_exists(&self, _path: &Path) -> RS<bool> {
            Ok(false)
        }

        async fn remove_file_if_exists(&self, _path: &Path) -> RS<()> {
            Ok(())
        }

        async fn read_dir(&self, _path: &Path) -> RS<Vec<PathBuf>> {
            Ok(vec![])
        }
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        use std::pin::Pin;
        use std::task::{Context, Poll};

        let waker = std::task::Waker::noop();
        let mut context = Context::from_waker(waker);
        let mut future: Pin<Box<F>> = Box::pin(future);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(value) => return value,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    #[test]
    fn remove_dir_all_default_returns_not_implemented() {
        let err = block_on(MockFs.remove_dir_all(Path::new("/tmp/x"))).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);
    }

    #[test]
    fn write_all_default_returns_not_implemented() {
        let err = block_on(MockFs.write_all(Path::new("/tmp/x"), b"data")).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);
    }

    #[test]
    fn read_to_string_default_uses_read_all() {
        let result = block_on(MockFs.read_to_string(Path::new("/tmp/x")));
        assert_eq!(result.unwrap(), "");
    }

    #[test]
    fn open_returns_file_for_default_mock() {
        let result = block_on(MockFs.open(Path::new("/tmp/x"), FileOptions::read_only()));
        assert!(result.is_ok());
    }

    #[test]
    fn create_dir_all_returns_ok_for_default_mock() {
        let result = block_on(MockFs.create_dir_all(Path::new("/tmp/x")));
        assert!(result.is_ok());
    }

    #[test]
    fn metadata_len_returns_zero_for_default_mock() {
        let result = block_on(MockFs.metadata_len(Path::new("/tmp/x")));
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn path_exists_returns_false_for_default_mock() {
        let result = block_on(MockFs.path_exists(Path::new("/tmp/x")));
        assert!(!result.unwrap());
    }

    #[test]
    fn remove_file_if_exists_returns_ok_for_default_mock() {
        let result = block_on(MockFs.remove_file_if_exists(Path::new("/tmp/x")));
        assert!(result.is_ok());
    }

    #[test]
    fn read_dir_returns_empty_for_default_mock() {
        let result = block_on(MockFs.read_dir(Path::new("/tmp/x")));
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn read_all_default_uses_open_and_file_len() {
        let result = block_on(MockFs.read_all(Path::new("/tmp/x")));
        assert!(result.unwrap().is_empty());
    }

    struct ContentMockFile {
        content: Vec<u8>,
    }

    #[async_trait]
    impl AsyncFile for ContentMockFile {
        async fn read_exact_at(&self, offset: u64, len: usize) -> RS<Vec<u8>> {
            let start = offset as usize;
            let end = (start + len).min(self.content.len());
            Ok(self.content[start..end].to_vec())
        }

        async fn write_all_at(&self, _offset: u64, _payload: &[u8]) -> RS<()> {
            Ok(())
        }

        async fn fsync(&self) -> RS<()> {
            Ok(())
        }

        async fn file_len(&self) -> RS<u64> {
            Ok(self.content.len() as u64)
        }
    }

    struct ContentMockFs {
        content: Vec<u8>,
    }

    #[async_trait]
    impl AsyncFs for ContentMockFs {
        async fn open(&self, _path: &Path, _options: FileOptions) -> RS<Arc<dyn AsyncFile>> {
            Ok(Arc::new(ContentMockFile {
                content: self.content.clone(),
            }))
        }

        async fn create_dir_all(&self, _path: &Path) -> RS<()> {
            Ok(())
        }

        async fn metadata_len(&self, _path: &Path) -> RS<u64> {
            Ok(self.content.len() as u64)
        }

        async fn path_exists(&self, _path: &Path) -> RS<bool> {
            Ok(true)
        }

        async fn remove_file_if_exists(&self, _path: &Path) -> RS<()> {
            Ok(())
        }

        async fn read_dir(&self, _path: &Path) -> RS<Vec<PathBuf>> {
            Ok(vec![])
        }
    }

    #[test]
    fn read_all_returns_full_content() {
        let fs = ContentMockFs {
            content: b"hello world".to_vec(),
        };
        let result = block_on(fs.read_all(Path::new("/tmp/x")));
        assert_eq!(result.unwrap(), b"hello world");
    }

    #[test]
    fn read_to_string_returns_utf8_content() {
        let fs = ContentMockFs {
            content: b"hello world".to_vec(),
        };
        let result = block_on(fs.read_to_string(Path::new("/tmp/x")));
        assert_eq!(result.unwrap(), "hello world");
    }

    #[test]
    fn read_to_string_rejects_invalid_utf8() {
        let fs = ContentMockFs {
            content: vec![0xff, 0xfe],
        };
        let err = block_on(fs.read_to_string(Path::new("/tmp/x"))).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidUtf8);
    }

    struct ErrorMockFs;

    #[async_trait]
    impl AsyncFs for ErrorMockFs {
        async fn open(&self, _path: &Path, _options: FileOptions) -> RS<Arc<dyn AsyncFile>> {
            Err(mudu::mudu_error!(ErrorCode::NotFound, "file not found"))
        }

        async fn create_dir_all(&self, _path: &Path) -> RS<()> {
            Ok(())
        }

        async fn metadata_len(&self, _path: &Path) -> RS<u64> {
            Ok(0)
        }

        async fn path_exists(&self, _path: &Path) -> RS<bool> {
            Ok(false)
        }

        async fn remove_file_if_exists(&self, _path: &Path) -> RS<()> {
            Ok(())
        }

        async fn read_dir(&self, _path: &Path) -> RS<Vec<PathBuf>> {
            Ok(vec![])
        }
    }

    #[test]
    fn read_all_propagates_open_error() {
        let err = block_on(ErrorMockFs.read_all(Path::new("/missing"))).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotFound);
    }

    #[test]
    fn read_to_string_propagates_open_error() {
        let err = block_on(ErrorMockFs.read_to_string(Path::new("/missing"))).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotFound);
    }

    struct FileLenErrorMockFile;

    #[async_trait]
    impl AsyncFile for FileLenErrorMockFile {
        async fn read_exact_at(&self, _offset: u64, _len: usize) -> RS<Vec<u8>> {
            Ok(vec![])
        }

        async fn write_all_at(&self, _offset: u64, _payload: &[u8]) -> RS<()> {
            Ok(())
        }

        async fn fsync(&self) -> RS<()> {
            Ok(())
        }

        async fn file_len(&self) -> RS<u64> {
            Err(mudu::mudu_error!(ErrorCode::Io, "file_len failed"))
        }
    }

    struct FileLenErrorMockFs;

    #[async_trait]
    impl AsyncFs for FileLenErrorMockFs {
        async fn open(&self, _path: &Path, _options: FileOptions) -> RS<Arc<dyn AsyncFile>> {
            Ok(Arc::new(FileLenErrorMockFile))
        }

        async fn create_dir_all(&self, _path: &Path) -> RS<()> {
            Ok(())
        }

        async fn metadata_len(&self, _path: &Path) -> RS<u64> {
            Ok(0)
        }

        async fn path_exists(&self, _path: &Path) -> RS<bool> {
            Ok(true)
        }

        async fn remove_file_if_exists(&self, _path: &Path) -> RS<()> {
            Ok(())
        }

        async fn read_dir(&self, _path: &Path) -> RS<Vec<PathBuf>> {
            Ok(vec![])
        }
    }

    #[test]
    fn read_all_propagates_file_len_error() {
        let err = block_on(FileLenErrorMockFs.read_all(Path::new("/bad"))).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Io);
    }
}
