use async_trait::async_trait;
use mudu::common::result::RS;

/// Async random-access file handle.
#[async_trait]
pub trait AsyncFile: Send + Sync {
    /// Read exactly `len` bytes starting at `offset`.
    async fn read_exact_at(&self, offset: u64, len: usize) -> RS<Vec<u8>>;

    /// Write all bytes of `payload` starting at `offset`.
    async fn write_all_at(&self, offset: u64, payload: &[u8]) -> RS<()>;

    /// Flush all written data to durable storage.
    async fn fsync(&self) -> RS<()>;

    /// Return the current file length in bytes.
    async fn file_len(&self) -> RS<u64>;

    /// Close the file, releasing any underlying resources.
    async fn close(&self) -> RS<()> {
        Ok(())
    }

    /// Return the raw file descriptor, if one exists.
    fn as_raw_fd(&self) -> Option<std::os::fd::RawFd> {
        None
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
    fn close_default_returns_ok() {
        let result = block_on(MockFile.close());
        assert!(result.is_ok());
    }

    #[test]
    fn as_raw_fd_default_returns_none() {
        assert!(MockFile.as_raw_fd().is_none());
    }

    #[test]
    fn read_exact_at_returns_empty_for_default_mock() {
        let result = block_on(MockFile.read_exact_at(0, 10));
        assert_eq!(result.unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn write_all_at_returns_ok_for_default_mock() {
        let result = block_on(MockFile.write_all_at(0, b"payload"));
        assert!(result.is_ok());
    }

    #[test]
    fn fsync_returns_ok_for_default_mock() {
        let result = block_on(MockFile.fsync());
        assert!(result.is_ok());
    }

    #[test]
    fn file_len_returns_zero_for_default_mock() {
        let result = block_on(MockFile.file_len());
        assert_eq!(result.unwrap(), 0);
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

    #[test]
    fn read_exact_at_returns_content_subset() {
        let file = ContentMockFile {
            content: b"hello world".to_vec(),
        };
        let result = block_on(file.read_exact_at(0, 5));
        assert_eq!(result.unwrap(), b"hello");
    }

    #[test]
    fn read_exact_at_clamps_to_end_of_file() {
        let file = ContentMockFile {
            content: b"hi".to_vec(),
        };
        let result = block_on(file.read_exact_at(0, 10));
        assert_eq!(result.unwrap(), b"hi");
    }

    #[test]
    fn file_len_reports_content_length() {
        let file = ContentMockFile {
            content: b"hello world".to_vec(),
        };
        let result = block_on(file.file_len());
        assert_eq!(result.unwrap(), 11);
    }

    #[test]
    fn default_methods_work_through_trait_object() {
        let file: Box<dyn AsyncFile> = Box::new(MockFile);
        assert!(block_on(file.close()).is_ok());
        assert!(file.as_raw_fd().is_none());
    }
}
