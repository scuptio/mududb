#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::imp::fs::async_io_uring::{
        iou_create_dir_all, iou_metadata_len, iou_path_exists, iou_remove_file_if_exists,
    };
    use crate::imp::native::linux::io_uring::test_ring::drive_iouring_future;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join("mududb-iou-create-dir-all")
            .join(format!("{name}-{}", crate::random::uuid_v4()))
    }

    fn cleanup(path: &Path) {
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn iou_create_dir_all_succeeds_when_directory_exists() {
        let path = temp_path("exists").join("a").join("b");
        cleanup(&path);
        std::fs::create_dir_all(&path).unwrap();

        let result = drive_iouring_future(iou_create_dir_all(&path));
        if let Some(result) = result {
            result.unwrap();
            assert!(path.is_dir());
        }

        cleanup(&path);
    }

    #[test]
    fn iou_create_dir_all_succeeds_when_directory_does_not_exist() {
        let root = temp_path("missing");
        let path = root.join("a").join("b").join("c");
        cleanup(&root);

        let result = drive_iouring_future(iou_create_dir_all(&path));
        if let Some(result) = result {
            result.unwrap();
            assert!(path.is_dir());
        }

        cleanup(&root);
    }

    #[test]
    fn iou_create_dir_all_succeeds_when_parent_partially_exists() {
        let root = temp_path("partial");
        let existing = root.join("a");
        let path = existing.join("b").join("c");
        cleanup(&root);
        std::fs::create_dir_all(&existing).unwrap();

        let result = drive_iouring_future(iou_create_dir_all(&path));
        if let Some(result) = result {
            result.unwrap();
            assert!(existing.is_dir());
            assert!(path.is_dir());
        }

        cleanup(&root);
    }

    #[test]
    fn iou_file_full_lifecycle() {
        use crate::contract::file_options::FileOptions;
        use crate::imp::fs::async_io_uring::io_uring_file::IoUringFile;
        use mudu::common::result::RS;

        let root = temp_path("file-lifecycle");
        cleanup(&root);
        let file_path = root.join("data.bin");

        let dir = root.clone();
        let path = file_path.clone();
        let result = drive_iouring_future(async move {
            iou_create_dir_all(&dir).await?;

            let file = IoUringFile::open(&path, FileOptions::read_write_create()).await?;
            file.write_all_at(0, b"hello-io-uring").await?;
            file.fsync().await?;
            assert_eq!(file.file_len().await?, 14);
            let data = file.read_exact_at(0, 14).await?;
            assert_eq!(data, b"hello-io-uring");
            let _fd = file.as_raw_fd();
            drop(file);

            assert!(iou_path_exists(&path).await?);
            assert_eq!(iou_metadata_len(&path).await?, 14);
            iou_remove_file_if_exists(&path).await?;
            assert!(!iou_path_exists(&path).await?);
            RS::Ok(())
        });
        if let Some(result) = result {
            result.unwrap();
        }
        cleanup(&root);
    }

    #[test]
    fn iou_file_split_submit_and_wait() {
        use crate::contract::file_options::FileOptions;
        use crate::imp::fs::async_io_uring::io_uring_file::{
            iou_flush_submit, iou_write_submit_option, IoUringFile,
        };
        use crate::imp::native::linux::io_uring::file::OptionWrite;
        use mudu::common::result::RS;

        let root = temp_path("split-submit");
        cleanup(&root);
        let file_path = root.join("split.bin");

        let dir = root.clone();
        let path = file_path.clone();
        let result = drive_iouring_future(async move {
            iou_create_dir_all(&dir).await?;
            let file = IoUringFile::open(&path, FileOptions::read_write_create()).await?;
            let write_handle =
                iou_write_submit_option(&file, b"ab".to_vec(), 0, OptionWrite::default())?;
            write_handle.wait().await?;
            let flush_handle = iou_flush_submit(&file)?;
            flush_handle.wait().await?;
            assert_eq!(file.file_len().await?, 2);
            RS::Ok(())
        });
        if let Some(result) = result {
            result.unwrap();
        }
        cleanup(&root);
    }

    #[test]
    fn async_iouring_fs_trait_surface() {
        use crate::contract::async_fs::AsyncFs;
        use crate::contract::file_options::FileOptions;
        use crate::imp::fs::async_io_uring::async_io_uring_fs::AsyncIoUringFs;
        use mudu::common::result::RS;

        let root = temp_path("fs-trait");
        cleanup(&root);
        let file_path = root.join("trait.bin");

        let dir = root.clone();
        let path = file_path.clone();
        let result = drive_iouring_future(async move {
            let fs = AsyncIoUringFs::new();
            fs.create_dir_all(&dir).await?;
            let file = fs.open(&path, FileOptions::read_write_create()).await?;
            file.write_all_at(0, b"xyz").await?;
            file.fsync().await?;
            assert_eq!(file.file_len().await?, 3);
            assert_eq!(file.read_exact_at(0, 3).await?, b"xyz");
            let _fd = file.as_raw_fd();
            drop(file);

            assert!(fs.path_exists(&path).await?);
            assert_eq!(fs.metadata_len(&path).await?, 3);
            let entries = fs.read_dir(&dir).await?;
            assert!(entries.iter().any(|p| p.ends_with("trait.bin")));
            fs.remove_file_if_exists(&path).await?;
            assert!(!fs.path_exists(&path).await?);
            fs.remove_dir_all(&dir).await?;
            RS::Ok(())
        });
        if let Some(result) = result {
            result.unwrap();
        }
        cleanup(&root);
    }
}
