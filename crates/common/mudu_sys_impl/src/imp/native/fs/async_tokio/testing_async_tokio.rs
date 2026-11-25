#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[cfg(test)]
mod tests {
    use crate::contract::file_options::FileOptions;
    use crate::imp::fs::async_tokio as async_;
    use project_root::get_project_root;
    use std::path::PathBuf;

    fn temp_path(name: &str) -> PathBuf {
        get_project_root()
            .unwrap()
            .join("target")
            .join("tmp")
            .join(format!("async-tokio-{name}-{}", crate::random::uuid_v4()))
    }

    #[tokio::test(flavor = "current_thread")]
    async fn tokio_fs_simulates_positioned_io() {
        let path = temp_path("positioned.dat");
        if let Some(parent) = path.parent() {
            async_::create_dir_all(parent).await.unwrap();
        }

        let file = async_::TokioFile::open(&path, FileOptions::read_write_create())
            .await
            .unwrap();
        file.write_all_at(4, b"bc").await.unwrap();
        file.write_all_at(0, b"a").await.unwrap();
        file.write_all_at(8, b"z").await.unwrap();
        file.fsync().await.unwrap();

        assert_eq!(file.read_exact_at(0, 1).await.unwrap(), b"a".to_vec());
        assert_eq!(file.read_exact_at(4, 2).await.unwrap(), b"bc".to_vec());
        assert_eq!(file.read_exact_at(8, 1).await.unwrap(), b"z".to_vec());

        async_::remove_file_if_exists(&path).await.unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn tokio_fs_remove_dir_all_removes_tree() {
        let root = temp_path("tree");
        let nested = root.join("a").join("b");
        async_::create_dir_all(&nested).await.unwrap();
        let file = async_::TokioFile::open(nested.join("f.dat"), FileOptions::read_write_create())
            .await
            .unwrap();
        file.write_all_at(0, b"payload").await.unwrap();
        file.fsync().await.unwrap();
        drop(file);
        assert!(async_::path_exists(&nested).await.unwrap());

        let fs = crate::imp::native::fs::tokio_fs();
        fs.remove_dir_all(&root).await.unwrap();
        assert!(!async_::path_exists(&root).await.unwrap());
    }
    #[tokio::test(flavor = "current_thread")]
    async fn truncate_and_atomic_publish_preserve_durable_contents() {
        let root = temp_path("publish");
        let fs = crate::imp::native::fs::tokio_fs();
        fs.create_dir_all(&root).await.unwrap();
        let temporary = root.join("snapshot.tmp");
        let published = root.join("snapshot");
        let file = fs
            .open(&temporary, FileOptions::read_write_create())
            .await
            .unwrap();
        file.write_all_at(0, b"validtorn").await.unwrap();
        file.set_len(5).await.unwrap();
        file.fsync().await.unwrap();
        fs.rename(&temporary, &published).await.unwrap();
        fs.sync_dir(&root).await.unwrap();
        assert_eq!(fs.read_all(&published).await.unwrap(), b"valid");
        assert!(!fs.path_exists(&temporary).await.unwrap());
        drop(file);
        fs.remove_dir_all(&root).await.unwrap();
    }
    #[tokio::test(flavor = "current_thread")]
    async fn filesystem_errors_and_directory_listing() {
        let fs = crate::imp::native::fs::tokio_fs();
        let root = temp_path("errors");
        let missing = root.join("missing");
        assert!(!fs.path_exists(&root).await.unwrap());
        assert!(fs.metadata_len(&missing).await.is_err());
        assert!(fs.read_dir(&missing).await.is_err());
        assert!(fs.sync_dir(&missing).await.is_err());
        assert!(fs.rename(&missing, &root).await.is_err());
        assert!(fs.remove_dir_all(&missing).await.is_err());
        assert!(fs.open(&missing, FileOptions::read_only()).await.is_err());
        fs.remove_file_if_exists(&missing).await.unwrap();
        fs.create_dir_all(&root).await.unwrap();
        assert!(fs.read_dir(&root).await.unwrap().is_empty());
        let path = root.join("data");
        let file = fs
            .open(&path, FileOptions::read_write_create())
            .await
            .unwrap();
        file.write_all_at(0, b"payload").await.unwrap();
        assert_eq!(file.file_len().await.unwrap(), 7);
        assert_eq!(fs.metadata_len(&path).await.unwrap(), 7);
        assert_eq!(fs.read_dir(&root).await.unwrap(), vec![path.clone()]);
        assert!(file.read_exact_at(0, 8).await.is_err());
        assert!(fs.create_dir_all(&path.join("child")).await.is_err());
        assert!(fs.read_dir(&path).await.is_err());
        assert!(fs.remove_file_if_exists(&root).await.is_err());
        drop(file);
        let readonly = fs.open(&path, FileOptions::read_only()).await.unwrap();
        assert!(readonly.write_all_at(0, b"bad").await.is_err());
        assert!(readonly.set_len(1).await.is_err());
        assert_eq!(readonly.read_exact_at(0, 7).await.unwrap(), b"payload");
        drop(readonly);
        fs.remove_file_if_exists(&path).await.unwrap();
        fs.remove_file_if_exists(&path).await.unwrap();
        fs.remove_dir_all(&root).await.unwrap();
    }
}
