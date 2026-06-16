use async_trait::async_trait;
use mudu::common::result::RS;

#[async_trait]
pub trait AsyncFile: Send + Sync {
    async fn read_exact_at(&self, offset: u64, len: usize) -> RS<Vec<u8>>;
    async fn write_all_at(&self, offset: u64, payload: &[u8]) -> RS<()>;
    async fn fsync(&self) -> RS<()>;
    async fn file_len(&self) -> RS<u64>;
    async fn close(&self) -> RS<()> {
        Ok(())
    }
    fn as_raw_fd(&self) -> Option<std::os::fd::RawFd> {
        None
    }
}
