use async_trait::async_trait;
use mudu::common::result::RS;

/// Async byte-stream abstraction.
#[async_trait]
pub trait AsyncStream: Send + Sync + Unpin {
    /// Read up to `buf.len()` bytes into `buf`.
    async fn read(&mut self, buf: &mut [u8]) -> RS<usize>;
    /// Write all bytes from `buf`.
    async fn write_all(&mut self, buf: &[u8]) -> RS<()>;
    /// Shut down the stream.
    async fn shutdown(&mut self) -> RS<()>;
    /// Return the raw file descriptor, if available.
    fn as_raw_fd(&self) -> Option<std::os::fd::RawFd>;
    /// Disable Nagle's algorithm on the underlying socket.
    fn set_nodelay(&self) -> RS<()>;
}
