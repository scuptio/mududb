use async_trait::async_trait;
use mudu::common::result::RS;
use crate::io::sys_file::SysFile;
use std::path::{Path, PathBuf};

#[async_trait]
pub trait SysFs: Send + Sync {
    async fn open(&self, path: &Path, flags: i32, mode: u32) -> RS<SysFile>;
    async fn read_exact_at(&self, file: &SysFile, len: usize, offset: u64) -> RS<Vec<u8>>;
    async fn write_all_at(&self, file: &SysFile, payload: &[u8], offset: u64) -> RS<()>;
    async fn fsync(&self, file: &SysFile) -> RS<()>;
    async fn close(&self, file: SysFile) -> RS<()>;

    async fn create_dir_all(&self, path: &Path) -> RS<()>;
    async fn read_dir(&self, path: &Path) -> RS<Vec<PathBuf>>;
    async fn metadata_len(&self, path: &Path) -> RS<u64>;
    async fn read_all(&self, path: &Path) -> RS<Vec<u8>>;
    async fn remove_file_if_exists(&self, path: &Path) -> RS<()>;
}
