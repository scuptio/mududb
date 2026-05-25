use crate::async_rt::contract::{AsyncFile, AsyncFs, FileOpenOptions};
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_sys::tokio::fs::{self, File, OpenOptions};
use mudu_sys::tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom};
use mudu_utils::sync::a_mutex::AMutex;
use std::io::ErrorKind;
use std::path::Path;
use std::sync::Arc;

#[derive(Default)]
pub struct TokioFs;

impl TokioFs {
    pub const fn new() -> Self {
        Self
    }
}

pub struct TokioFile {
    inner: AMutex<File>,
}

impl TokioFile {
    pub fn new(file: File) -> Self {
        Self {
            inner: AMutex::new(file),
        }
    }
}

#[async_trait]
impl AsyncFs for TokioFs {
    async fn open(&self, path: &Path, options: FileOpenOptions) -> RS<Arc<dyn AsyncFile>> {
        let mut open = OpenOptions::new();
        open.read(options.read);
        open.write(options.write || options.append);
        open.create(options.create);
        open.truncate(options.truncate);
        open.append(options.append);
        open.create_new(options.create_new);
        let file = open
            .open(path)
            .await
            .map_err(|e| m_error!(EC::IOErr, "open tokio file error", e))?;
        Ok(Arc::new(TokioFile::new(file)))
    }

    async fn create_dir_all(&self, path: &Path) -> RS<()> {
        fs::create_dir_all(path)
            .await
            .map_err(|e| m_error!(EC::IOErr, "create tokio directory error", e))
    }

    async fn metadata_len(&self, path: &Path) -> RS<u64> {
        fs::metadata(path)
            .await
            .map(|metadata| metadata.len())
            .map_err(|e| m_error!(EC::IOErr, "read tokio metadata error", e))
    }

    async fn path_exists(&self, path: &Path) -> RS<bool> {
        fs::try_exists(path)
            .await
            .map_err(|e| m_error!(EC::IOErr, "check tokio path exists error", e))
    }

    async fn remove_file_if_exists(&self, path: &Path) -> RS<()> {
        match fs::remove_file(path).await {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
            Err(err) => Err(m_error!(EC::IOErr, "remove tokio file error", err)),
        }
    }
}

#[async_trait]
impl AsyncFile for TokioFile {
    async fn read_exact_at(&self, offset: u64, len: usize) -> RS<Vec<u8>> {
        let mut file = self.inner.lock().await;
        file.seek(SeekFrom::Start(offset))
            .await
            .map_err(|e| m_error!(EC::IOErr, "seek tokio file for read error", e))?;
        let mut buf = vec![0u8; len];
        file.read_exact(&mut buf)
            .await
            .map_err(|e| m_error!(EC::IOErr, "read tokio file error", e))?;
        Ok(buf)
    }

    async fn write_all_at(&self, offset: u64, payload: &[u8]) -> RS<()> {
        let mut file = self.inner.lock().await;
        file.seek(SeekFrom::Start(offset))
            .await
            .map_err(|e| m_error!(EC::IOErr, "seek tokio file for write error", e))?;
        file.write_all(payload)
            .await
            .map_err(|e| m_error!(EC::IOErr, "write tokio file error", e))
    }

    async fn fsync(&self) -> RS<()> {
        let file = self.inner.lock().await;
        file.sync_all()
            .await
            .map_err(|e| m_error!(EC::IOErr, "fsync tokio file error", e))
    }

    async fn file_len(&self) -> RS<u64> {
        let file = self.inner.lock().await;
        file.metadata()
            .await
            .map(|metadata| metadata.len())
            .map_err(|e| m_error!(EC::IOErr, "read tokio file metadata error", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use project_root::get_project_root;
    use std::path::PathBuf;

    fn temp_path(name: &str) -> PathBuf {
        get_project_root()
            .unwrap()
            .join("target")
            .join("tmp")
            .join(format!("tokio-fs-{name}-{}", mudu_sys::random::uuid_v4()))
    }

    #[tokio::test(flavor = "current_thread")]
    async fn tokio_fs_simulates_positioned_io() {
        let fs = TokioFs::new();
        let path = temp_path("positioned.dat");
        if let Some(parent) = path.parent() {
            fs.create_dir_all(parent).await.unwrap();
        }

        let file = fs
            .open(&path, FileOpenOptions::read_write_create())
            .await
            .unwrap();
        file.write_all_at(4, b"bc").await.unwrap();
        file.write_all_at(0, b"a").await.unwrap();
        file.write_all_at(8, b"z").await.unwrap();
        file.fsync().await.unwrap();

        assert_eq!(file.read_exact_at(0, 1).await.unwrap(), b"a".to_vec());
        assert_eq!(file.read_exact_at(4, 2).await.unwrap(), b"bc".to_vec());
        assert_eq!(file.read_exact_at(8, 1).await.unwrap(), b"z".to_vec());

        fs.remove_file_if_exists(&path).await.unwrap();
    }
}
