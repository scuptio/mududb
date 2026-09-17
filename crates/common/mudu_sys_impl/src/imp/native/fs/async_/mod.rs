#![allow(missing_docs)]
use crate::contract::file_options::FileOptions;
use crate::env::default_env;
use crate::scoped_task_trace;
use mudu::common::result::RS;
use std::path::{Path, PathBuf};

use super::sys_file::SysFile;

pub async fn open(path: impl AsRef<Path>, flags: i32, mode: u32) -> RS<SysFile> {
    let file = default_env()
        .provider()
        .fs()
        .open(path.as_ref(), FileOptions::new(flags, mode))
        .await?;
    Ok(SysFile::new(file))
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
    default_env().provider().fs().create_dir_all(path).await
}

pub async fn read_dir(path: &Path) -> RS<Vec<PathBuf>> {
    scoped_task_trace!();
    default_env().provider().fs().read_dir(path).await
}

pub async fn metadata_len(path: &Path) -> RS<u64> {
    default_env().provider().fs().metadata_len(path).await
}

pub async fn remove_file_if_exists(path: &Path) -> RS<()> {
    default_env()
        .provider()
        .fs()
        .remove_file_if_exists(path)
        .await
}

pub async fn read_all(path: &Path) -> RS<Vec<u8>> {
    default_env().provider().fs().read_all(path).await
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_path(name: &str) -> PathBuf {
        project_root::get_project_root()
            .unwrap()
            .join("target")
            .join("tmp")
            .join(format!("async-facade-{name}-{}", crate::random::uuid_v4()))
    }

    #[tokio::test(flavor = "current_thread")]
    async fn async_fs_create_dir_all_and_read_dir() {
        let base = temp_path("create-dir");
        let nested = base.join("nested");
        create_dir_all(&nested).await.unwrap();

        let entries = read_dir(&base).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].ends_with("nested"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn async_fs_open_write_read() {
        let path = temp_path("open-write-read.dat");
        if let Some(parent) = path.parent() {
            create_dir_all(parent).await.unwrap();
        }

        let file = open(&path, libc::O_RDWR | libc::O_CREAT, 0o644)
            .await
            .unwrap();
        write_all_at(&file, b"bc", 4).await.unwrap();
        write_all_at(&file, b"a", 0).await.unwrap();
        write_all_at(&file, b"z", 8).await.unwrap();

        assert_eq!(read_exact_at(&file, 1, 0).await.unwrap(), b"a".to_vec());
        assert_eq!(read_exact_at(&file, 2, 4).await.unwrap(), b"bc".to_vec());
        assert_eq!(read_exact_at(&file, 1, 8).await.unwrap(), b"z".to_vec());

        close(file).await.unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn async_fs_read_all() {
        let path = temp_path("read-all.dat");
        if let Some(parent) = path.parent() {
            create_dir_all(parent).await.unwrap();
        }

        let contents = b"hello async facade".to_vec();
        let file = open(&path, libc::O_RDWR | libc::O_CREAT, 0o644)
            .await
            .unwrap();
        write_all_at(&file, &contents, 0).await.unwrap();
        fsync(&file).await.unwrap();
        close(file).await.unwrap();

        let read = read_all(&path).await.unwrap();
        assert_eq!(read, contents);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn async_fs_fsync_and_close() {
        let path = temp_path("fsync-close.dat");
        if let Some(parent) = path.parent() {
            create_dir_all(parent).await.unwrap();
        }

        let contents = b"12345";
        let file = open(&path, libc::O_RDWR | libc::O_CREAT, 0o644)
            .await
            .unwrap();
        write_all_at(&file, contents, 0).await.unwrap();
        fsync(&file).await.unwrap();
        close(file).await.unwrap();

        let len = metadata_len(&path).await.unwrap();
        assert_eq!(len, contents.len() as u64);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn async_fs_remove_file_if_exists() {
        let path = temp_path("remove-file.dat");
        if let Some(parent) = path.parent() {
            create_dir_all(parent).await.unwrap();
        }

        let file = open(&path, libc::O_RDWR | libc::O_CREAT, 0o644)
            .await
            .unwrap();
        close(file).await.unwrap();

        remove_file_if_exists(&path).await.unwrap();
        assert!(metadata_len(&path).await.is_err());
        remove_file_if_exists(&path).await.unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn async_fs_metadata_len() {
        let path = temp_path("metadata-len.dat");
        if let Some(parent) = path.parent() {
            create_dir_all(parent).await.unwrap();
        }

        let contents = b"known payload";
        let file = open(&path, libc::O_RDWR | libc::O_CREAT, 0o644)
            .await
            .unwrap();
        write_all_at(&file, contents, 0).await.unwrap();
        fsync(&file).await.unwrap();
        close(file).await.unwrap();

        assert_eq!(metadata_len(&path).await.unwrap(), contents.len() as u64);
    }
}
