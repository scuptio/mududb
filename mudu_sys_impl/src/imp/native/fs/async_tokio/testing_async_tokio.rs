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
}
