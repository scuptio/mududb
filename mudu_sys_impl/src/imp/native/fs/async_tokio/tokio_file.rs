use crate::contract::file_options::FileOptions;
use crate::imp::sync::async_::AMutex;
use mudu::common::result::RS;
use mudu::error::others::io_error_with_message;
use mudu::error::ErrorCode;
use mudu::mudu_error;
#[cfg(unix)]
use std::os::fd::{AsRawFd, RawFd};
use std::path::Path;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom};

pub struct TokioFile {
    inner: AMutex<fs::File>,
}

impl TokioFile {
    fn from(file: fs::File) -> Self {
        Self {
            inner: AMutex::new(file),
        }
    }
    async fn open_inner(path: impl AsRef<Path>, options: FileOptions) -> RS<TokioFile> {
        let mut open = fs::OpenOptions::new();
        open.read(options.read);
        open.write(options.write || options.append);
        open.create(options.create);
        open.truncate(options.truncate);
        open.append(options.append);
        open.create_new(options.create_new);
        let file = open.open(path.as_ref()).await.map_err(|e| {
            mudu_error!(
                ErrorCode::from(&e),
                format!("open file error, path {}", path.as_ref().to_string_lossy()),
                e
            )
        })?;
        Ok(Self::from(file))
    }

    pub(crate) async fn open(path: impl AsRef<Path>, options: FileOptions) -> RS<TokioFile> {
        Self::open_inner(path, options).await
    }

    pub(crate) async fn read_exact_at(&self, offset: u64, len: usize) -> RS<Vec<u8>> {
        let mut file = self.inner.lock().await;
        file.seek(SeekFrom::Start(offset))
            .await
            .map_err(|e| io_error_with_message(e, "seek tokio file for read error"))?;
        let mut buf = vec![0u8; len];
        file.read_exact(&mut buf)
            .await
            .map_err(|e| io_error_with_message(e, "read tokio file error"))?;
        Ok(buf)
    }

    pub(crate) async fn write_all_at(&self, offset: u64, payload: &[u8]) -> RS<()> {
        let mut file = self.inner.lock().await;
        file.seek(SeekFrom::Start(offset))
            .await
            .map_err(|e| io_error_with_message(e, "seek tokio file for write error"))?;
        file.write_all(payload)
            .await
            .map_err(|e| io_error_with_message(e, "write tokio file error"))
    }

    pub(crate) async fn fsync(&self) -> RS<()> {
        let file = self.inner.lock().await;
        file.sync_all()
            .await
            .map_err(|e| io_error_with_message(e, "fsync tokio file error"))
    }

    pub(crate) async fn file_len(&self) -> RS<u64> {
        let file = self.inner.lock().await;
        file.metadata()
            .await
            .map(|metadata| metadata.len())
            .map_err(|e| io_error_with_message(e, "read tokio file metadata error"))
    }

    #[cfg(unix)]
    pub(crate) fn as_raw_fd(&self) -> Option<RawFd> {
        self.inner.try_lock().map(|file| file.as_raw_fd())
    }
}
