use crate::contract::file_options::FileOptions;
use crate::env::default_env;
use crate::io::sys_file::SysFile;
use crate::scoped_task_trace;
use mudu::common::result::RS;
use std::path::{Path, PathBuf};

const USE_ENV: bool = true;
pub async fn open(path: impl AsRef<Path>, flags: i32, mode: u32) -> RS<SysFile> {
    if USE_ENV {
        let file = default_env()
            .provider()
            .fs()
            .open(path.as_ref(), FileOptions::new(flags, mode))
            .await?;
        Ok(SysFile::new(file))
    } else {
        let file = default_env()
            .tokio_provider()
            .fs()
            .open(path.as_ref(), FileOptions::new(flags, mode))
            .await?;
        Ok(SysFile::new(file))
    }
}

pub async fn read_exact_at(file: &SysFile, len: usize, offset: u64) -> RS<Vec<u8>> {
    file.read_exact_at(offset, len).await
}

pub async fn write_all_at(file: &SysFile, payload: &[u8], offset: u64) -> RS<()> {
    file.write_all_at(offset, payload).await
}

pub async fn fsync(file: &SysFile) -> RS<()> {
    file.fsync().await
}

pub async fn close(file: SysFile) -> RS<()> {
    file.close().await
}

pub async fn create_dir_all(path: &Path) -> RS<()> {
    scoped_task_trace!();
    if USE_ENV {
        default_env().provider().fs().create_dir_all(path).await
    } else {
        default_env()
            .tokio_provider()
            .fs()
            .create_dir_all(path)
            .await
    }
}

pub async fn read_dir(path: &Path) -> RS<Vec<PathBuf>> {
    scoped_task_trace!();
    if USE_ENV {
        default_env().provider().fs().read_dir(path).await
    } else {
        default_env().tokio_provider().fs().read_dir(path).await
    }
}

pub async fn metadata_len(path: &Path) -> RS<u64> {
    if USE_ENV {
        default_env().provider().fs().metadata_len(path).await
    } else {
        default_env().tokio_provider().fs().metadata_len(path).await
    }
}

pub async fn read_all(path: &Path) -> RS<Vec<u8>> {
    if USE_ENV {
        default_env().provider().fs().read_all(path).await
    } else {
        default_env().tokio_provider().fs().read_all(path).await
    }
}

pub async fn remove_file_if_exists(path: &Path) -> RS<()> {
    if USE_ENV {
        default_env()
            .provider()
            .fs()
            .remove_file_if_exists(path)
            .await
    } else {
        default_env()
            .tokio_provider()
            .fs()
            .remove_file_if_exists(path)
            .await
    }
}
