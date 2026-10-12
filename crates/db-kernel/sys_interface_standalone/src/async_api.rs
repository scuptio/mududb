use mudu::common::result::RS;
use mudu_binding::system::{command_invoke, query_invoke};
use mudu_contract::database::result_batch::ResultBatch;
use sys_interface::host;

pub use crate::async_impl::*;
/// Re-export the fs data types so this module exposes the same API surface as
/// `sys_interface::async_api`.
pub use sys_interface::fs::{FsDirEntry, FsStat};

/// Execute a query from a serialized byte payload, materializing all rows.
pub async fn mudu_query_bytes(query_in: &[u8]) -> RS<Vec<u8>> {
    let (oid, stmt, params) = query_invoke::deserialize_query_param(query_in)?;
    let result = mudu_query::<String>(oid, stmt.as_ref(), params.as_ref())
        .await
        .and_then(|set| {
            let (rows, desc) = set.into_parts();
            Ok((
                ResultBatch::from_result_set(oid, rows.as_ref())?,
                desc.as_ref().clone(),
            ))
        });
    Ok(query_invoke::serialize_query_result(result))
}

/// The adapter backend materializes query results in `mudu_query_bytes`, so
/// there is no cursor to fetch further rows from.
pub async fn mudu_fetch_bytes(_cursor: &[u8]) -> RS<Vec<u8>> {
    Err(mudu::mudu_error!(
        mudu::error::ErrorCode::NotImplemented,
        "adapter backend materializes query results; no cursor to fetch"
    ))
}

/// Execute a command from a serialized byte payload.
pub async fn mudu_command_bytes(command_in: &[u8]) -> RS<Vec<u8>> {
    let (oid, stmt, params) = command_invoke::deserialize_command_param(command_in)?;
    Ok(command_invoke::serialize_command_result(
        mudu_command(oid, stmt.as_ref(), params.as_ref()).await,
    ))
}

/// Execute a batch of statements from a serialized byte payload.
pub async fn mudu_batch_bytes(batch_in: &[u8]) -> RS<Vec<u8>> {
    let (oid, stmt, params) = command_invoke::deserialize_command_param(batch_in)?;
    Ok(command_invoke::serialize_command_result(
        mudu_batch(oid, stmt.as_ref(), params.as_ref()).await,
    ))
}

/// Open a new session from a serialized byte payload.
pub async fn mudu_open_bytes(open_in: &[u8]) -> RS<Vec<u8>> {
    let argv = host::deserialize_open_param(open_in)?;
    Ok(host::serialize_open_result(mudu_open_argv(&argv).await?))
}

/// Close a session from a serialized byte payload.
pub async fn mudu_close_bytes(close_in: &[u8]) -> RS<Vec<u8>> {
    let session_id = host::deserialize_close_param(close_in)?;
    mudu_close(session_id).await?;
    Ok(host::serialize_close_result())
}

/// Get a value by key from a serialized byte payload.
pub async fn mudu_get_bytes(get_in: &[u8]) -> RS<Vec<u8>> {
    let (session_id, key) = host::deserialize_session_get_param(get_in)?;
    let value = mudu_get(session_id, &key).await?;
    Ok(host::serialize_get_result(value.as_deref()))
}

/// Store a key-value pair from a serialized byte payload.
pub async fn mudu_put_bytes(put_in: &[u8]) -> RS<Vec<u8>> {
    let (session_id, key, value) = host::deserialize_session_put_param(put_in)?;
    mudu_put(session_id, &key, &value).await?;
    Ok(host::serialize_put_result())
}

/// Scan a key range from a serialized byte payload.
pub async fn mudu_range_bytes(range_in: &[u8]) -> RS<Vec<u8>> {
    let (session_id, start_key, end_key) = host::deserialize_session_range_param(range_in)?;
    let items = mudu_range(session_id, &start_key, &end_key).await?;
    Ok(host::serialize_range_result(&items))
}

// These tests exercise the SQLite-backed adapter, which is unsupported under Miri.
#[cfg(all(test, not(miri)))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::next_oid;

    fn temp_db_path(name: &str) -> std::path::PathBuf {
        let suffix = mudu_sys::time::system_time_now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        mudu_sys::env_var::temp_dir().join(format!("sys_interface_async_{name}_{suffix}.db"))
    }

    fn run_adapter_test<F, Fut>(name: &str, f: F)
    where
        F: FnOnce(std::path::PathBuf) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let _guard = mudu_adapter::config::test_lock()
            .lock()
            .expect("test lock poisoned");
        let db_path = temp_db_path(name);
        mudu_adapter::syscall::set_db_path(&db_path);
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime")
            .block_on(f(db_path));
    }

    #[test]
    fn mudu_open_bytes_roundtrips_session_id() {
        run_adapter_test("open_bytes", |db_path| async move {
            let _ = db_path;
            let output = mudu_open_bytes(&host::serialize_open_param())
                .await
                .unwrap();
            let session_id = host::deserialize_open_result(&output).unwrap();
            assert!(session_id > 0);
        });
    }

    #[test]
    fn mudu_close_bytes_rejects_missing_session() {
        run_adapter_test("close_bytes", |_db_path| async move {
            let oid = next_oid();
            let err = mudu_close_bytes(&host::serialize_close_param(oid))
                .await
                .unwrap_err();
            assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
        });
    }

    #[test]
    fn mudu_get_bytes_rejects_missing_session() {
        run_adapter_test("get_bytes", |_db_path| async move {
            let oid = next_oid();
            let err = mudu_get_bytes(&host::serialize_session_get_param(oid, b"k"))
                .await
                .unwrap_err();
            assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
        });
    }

    #[test]
    fn mudu_put_bytes_rejects_missing_session() {
        run_adapter_test("put_bytes", |_db_path| async move {
            let oid = next_oid();
            let err = mudu_put_bytes(&host::serialize_session_put_param(oid, b"k", b"v"))
                .await
                .unwrap_err();
            assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
        });
    }

    #[test]
    fn mudu_range_bytes_rejects_missing_session() {
        run_adapter_test("range_bytes", |_db_path| async move {
            let oid = next_oid();
            let err = mudu_range_bytes(&host::serialize_session_range_param(oid, b"a", b"z"))
                .await
                .unwrap_err();
            assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
        });
    }

    #[test]
    fn mudu_fetch_bytes_reports_not_implemented() {
        let err = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime")
            .block_on(mudu_fetch_bytes(b""))
            .unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::NotImplemented);
    }
}
