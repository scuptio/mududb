#[cfg(test)]
mod tests {
    use crate::backend::mududb_cfg::ServerMode;
    use crate::service::runtime_impl::create_runtime_service;
    use crate::service::runtime_opt::RuntimeOpt;
    use crate::service::test_wasm_mod_path::wasm_mod_path;
    use mudu_utils::notifier::notify_wait;
    use std::env::temp_dir;

    use std::path::PathBuf;
    use mudu_sys::contract::async_mode::AsyncMode;

    fn temp_path(prefix: &str) -> PathBuf {
        temp_dir().join(format!("{}_{}", prefix, mudu_sys::random::uuid_v4()))
    }

    #[test]
    fn create_runtime_service_rejects_file_db_path() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let package_path = wasm_mod_path();
        let db_file = temp_path("runtime_impl_db_file");
        mudu_sys::fs::sync::write(&db_file, b"not-a-directory").unwrap();

        let err = match create_runtime_service(
            &package_path,
            &db_file.to_string_lossy().to_string(),
            None,
            RuntimeOpt::default(),
        )
        .await
        {
            Ok(_) => panic!("expected invalid db path error"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("is not a directory"));
        mudu_sys::fs::sync::remove_file(db_file).unwrap();
        }).unwrap()
    }

    #[test]
    fn create_runtime_service_notifies_after_initialization() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let package_dir = temp_path("runtime_impl_pkg_dir");
        let db_path = temp_path("runtime_impl_db_dir");
        mudu_sys::fs::sync::create_dir_all(&package_dir).unwrap();
        let (notifier, waiter) = notify_wait();

        let runtime = create_runtime_service(
            &package_dir.to_string_lossy().to_string(),
            &db_path.to_string_lossy().to_string(),
            Some(notifier),
            RuntimeOpt::default(),
        )
        .await
        .unwrap();

        waiter.wait().await;
        assert!(runtime.list().await.is_empty());

        let _ = mudu_sys::fs::sync::remove_dir_all(package_dir);
        let _ = mudu_sys::fs::sync::remove_dir_all(db_path);
        }).unwrap()
    }

    #[test]
    fn create_runtime_service_injects_tokio_async_runtime() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let package_dir = temp_path("runtime_impl_tokio_pkg_dir");
        let db_path = temp_path("runtime_impl_tokio_db_dir");
        mudu_sys::fs::sync::create_dir_all(&package_dir).unwrap();

        let runtime = create_runtime_service(
            &package_dir.to_string_lossy().to_string(),
            &db_path.to_string_lossy().to_string(),
            None,
            RuntimeOpt {
                sever_mode: ServerMode::Tokio,
                async_runtime: RuntimeOpt::build_async_runtime(ServerMode::Tokio),
                ..RuntimeOpt::default()
            },
        )
        .await
        .unwrap();

        let async_runtime = runtime.async_runtime().expect("tokio async runtime");
        assert!(matches!(
            async_runtime.mode(),
            AsyncMode::Tokio
        ));

        let _ = mudu_sys::fs::sync::remove_dir_all(package_dir);
        let _ = mudu_sys::fs::sync::remove_dir_all(db_path);
        }).unwrap()
    }
}
