use crate::common::std_file::StdAsyncFile;
use crate::contract::async_file::AsyncFile;
use mudu::common::result::RS;
#[cfg(unix)]
use std::os::fd::RawFd;
use std::sync::Arc;

#[derive(Clone)]
pub struct SysFile {
    inner: Arc<dyn AsyncFile>,
}

impl SysFile {
    pub fn new(inner: Arc<dyn AsyncFile>) -> Self {
        Self { inner }
    }

    pub fn from_std(file: std::fs::File) -> Self {
        Self::new(Arc::new(StdAsyncFile::new(file)))
    }

    pub async fn read_exact_at(&self, offset: u64, len: usize) -> RS<Vec<u8>> {
        self.inner.read_exact_at(offset, len).await
    }
    pub async fn write_all_at(&self, offset: u64, payload: &[u8]) -> RS<()> {
        self.inner.write_all_at(offset, payload).await
    }

    pub async fn fsync(&self) -> RS<()> {
        self.inner.fsync().await
    }

    pub async fn file_len(&self) -> RS<u64> {
        self.inner.file_len().await
    }

    pub async fn close(&self) -> RS<()> {
        self.inner.close().await
    }

    pub fn as_raw_fd(&self) -> Option<RawFd> {
        self.inner.as_raw_fd()
    }
}
