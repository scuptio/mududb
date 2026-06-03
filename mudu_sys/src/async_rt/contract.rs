use crate::async_rt::mode::AsyncMode;
use async_trait::async_trait;
use mudu::common::result::RS;
use std::path::Path;
use std::sync::Arc;

pub trait AsyncRuntime: Send + Sync {
    fn mode(&self) -> AsyncMode;
    fn net(&self) -> &dyn AsyncNet;
    fn fs(&self) -> &dyn AsyncFs;
    fn fs_arc(&self) -> Arc<dyn AsyncFs>;
}

#[async_trait]
pub trait AsyncNet: Send + Sync {
    async fn bind_tcp(&self, _addr: std::net::SocketAddr) -> RS<Arc<dyn AsyncListener>> {
        Err(mudu::m_error!(
            mudu::error::ec::EC::NotImplemented,
            "async net bind_tcp is not implemented"
        ))
    }

    async fn connect_tcp(&self, _addr: std::net::SocketAddr) -> RS<Box<dyn AsyncStream>> {
        Err(mudu::m_error!(
            mudu::error::ec::EC::NotImplemented,
            "async net connect_tcp is not implemented"
        ))
    }
}

#[async_trait]
pub trait AsyncListener: Send + Sync {
    fn local_addr(&self) -> RS<std::net::SocketAddr>;
    async fn accept(&self) -> RS<(Box<dyn AsyncStream>, std::net::SocketAddr)>;
}

#[async_trait]
pub trait AsyncStream: Send + Sync + Unpin {
    async fn read(&mut self, buf: &mut [u8]) -> RS<usize>;
    async fn write_all(&mut self, buf: &[u8]) -> RS<()>;
    async fn shutdown(&mut self) -> RS<()>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FileOpenOptions {
    pub read: bool,
    pub write: bool,
    pub create: bool,
    pub truncate: bool,
    pub append: bool,
    pub create_new: bool,
}

impl FileOpenOptions {
    pub const fn read_only() -> Self {
        Self {
            read: true,
            write: false,
            create: false,
            truncate: false,
            append: false,
            create_new: false,
        }
    }

    pub const fn read_write_create() -> Self {
        Self {
            read: true,
            write: true,
            create: true,
            truncate: false,
            append: false,
            create_new: false,
        }
    }
}

#[async_trait]
pub trait AsyncFs: Send + Sync {
    async fn open(&self, path: &Path, options: FileOpenOptions) -> RS<Arc<dyn AsyncFile>>;
    async fn create_dir_all(&self, path: &Path) -> RS<()>;
    async fn metadata_len(&self, path: &Path) -> RS<u64>;
    async fn path_exists(&self, path: &Path) -> RS<bool>;
    async fn remove_file_if_exists(&self, path: &Path) -> RS<()>;

    async fn read_all(&self, path: &Path) -> RS<Vec<u8>> {
        let file = self.open(path, FileOpenOptions::read_only()).await?;
        let len = file.file_len().await?;
        file.read_exact_at(0, len as usize).await
    }
}

#[async_trait]
pub trait AsyncFile: Send + Sync {
    async fn read_exact_at(&self, offset: u64, len: usize) -> RS<Vec<u8>>;
    async fn write_all_at(&self, offset: u64, payload: &[u8]) -> RS<()>;
    async fn fsync(&self) -> RS<()>;
    async fn file_len(&self) -> RS<u64>;
    fn as_raw_fd(&self) -> Option<std::os::fd::RawFd> {
        None
    }
}
