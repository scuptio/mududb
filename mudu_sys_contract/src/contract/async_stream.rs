use async_trait::async_trait;
use mudu::common::result::RS;

#[async_trait]
pub trait AsyncStream: Send + Sync + Unpin {
    async fn read(&mut self, buf: &mut [u8]) -> RS<usize>;
    async fn write_all(&mut self, buf: &[u8]) -> RS<()>;
    async fn shutdown(&mut self) -> RS<()>;
    fn as_raw_fd(&self) -> Option<std::os::fd::RawFd>;
    fn set_nodelay(&self) -> RS<()>;
}
