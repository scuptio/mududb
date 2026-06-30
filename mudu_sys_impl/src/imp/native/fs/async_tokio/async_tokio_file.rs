use crate::contract::async_file::AsyncFile;
use crate::contract::file_options::FileOptions;
use crate::imp::fs::async_tokio::TokioFile;
use async_trait::async_trait;
use mudu::common::result::RS;
use std::os::fd::RawFd;
use std::path::Path;

pub struct AsyncTokioFile {
    inner: TokioFile,
}

impl AsyncTokioFile {
    pub(crate) async fn open(path: impl AsRef<Path>, options: FileOptions) -> RS<Self> {
        Ok(Self {
            inner: TokioFile::open(path, options).await?,
        })
    }
}

#[async_trait]
impl AsyncFile for AsyncTokioFile {
    async fn read_exact_at(&self, offset: u64, len: usize) -> RS<Vec<u8>> {
        self.inner.read_exact_at(offset, len).await
    }

    async fn write_all_at(&self, offset: u64, payload: &[u8]) -> RS<()> {
        self.inner.write_all_at(offset, payload).await
    }

    async fn fsync(&self) -> RS<()> {
        self.inner.fsync().await
    }

    async fn file_len(&self) -> RS<u64> {
        self.inner.file_len().await
    }

    #[cfg(unix)]
    fn as_raw_fd(&self) -> Option<RawFd> {
        self.inner.as_raw_fd()
    }
}
