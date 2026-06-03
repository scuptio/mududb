use crate::env::default_env;
use crate::io::sys_file::SysFile;
use mudu::common::result::RS;
use std::path::{Path, PathBuf};

pub async fn open(path: &Path, flags: i32, mode: u32) -> RS<SysFile> {
    default_env().fs().open(path, flags, mode).await
}

pub async fn read_exact_at(file: &SysFile, len: usize, offset: u64) -> RS<Vec<u8>> {
    default_env().fs().read_exact_at(file, len, offset).await
}

pub async fn write_all_at(file: &SysFile, payload: &[u8], offset: u64) -> RS<()> {
    default_env().fs().write_all_at(file, payload, offset).await
}

pub async fn fsync(file: &SysFile) -> RS<()> {
    default_env().fs().fsync(file).await
}

pub async fn close(file: SysFile) -> RS<()> {
    default_env().fs().close(file).await
}

pub async fn create_dir_all(path: &Path) -> RS<()> {
    default_env().fs().create_dir_all(path).await
}

pub async fn read_dir(path: &Path) -> RS<Vec<PathBuf>> {
    default_env().fs().read_dir(path).await
}

pub async fn metadata_len(path: &Path) -> RS<u64> {
    default_env().fs().metadata_len(path).await
}

pub async fn read_all(path: &Path) -> RS<Vec<u8>> {
    default_env().fs().read_all(path).await
}

pub async fn remove_file_if_exists(path: &Path) -> RS<()> {
    default_env().fs().remove_file_if_exists(path).await
}
