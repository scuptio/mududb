use mudu::common::result::RS;
use sys_interface::host;

pub use crate::sync_impl::*;
/// Re-export the fs data types so this module exposes the same API surface as
/// `sys_interface::sync_api`.
pub use sys_interface::fs::{FsDirEntry, FsStat};
/// Re-export the backend-independent byte-level entry points from
/// `sys_interface`.
pub use sys_interface::sync_api::{
    mudu_batch_bytes, mudu_command_bytes, mudu_fetch_bytes, mudu_query_bytes,
};

/// Open a new session from a serialized byte payload.
pub fn mudu_open_bytes(open_in: &[u8]) -> RS<Vec<u8>> {
    let argv = host::deserialize_open_param(open_in)?;
    Ok(host::serialize_open_result(mudu_open_argv(&argv)?))
}

/// Close a session from a serialized byte payload.
pub fn mudu_close_bytes(close_in: &[u8]) -> RS<Vec<u8>> {
    let session_id = host::deserialize_close_param(close_in)?;
    mudu_close(session_id)?;
    Ok(host::serialize_close_result())
}

/// Get a value by key from a serialized byte payload.
pub fn mudu_get_bytes(get_in: &[u8]) -> RS<Vec<u8>> {
    let (session_id, key) = host::deserialize_session_get_param(get_in)?;
    let value = mudu_get(session_id, &key)?;
    Ok(host::serialize_get_result(value.as_deref()))
}

/// Store a key-value pair from a serialized byte payload.
pub fn mudu_put_bytes(put_in: &[u8]) -> RS<Vec<u8>> {
    let (session_id, key, value) = host::deserialize_session_put_param(put_in)?;
    mudu_put(session_id, &key, &value)?;
    Ok(host::serialize_put_result())
}

/// Scan a key range from a serialized byte payload.
pub fn mudu_range_bytes(range_in: &[u8]) -> RS<Vec<u8>> {
    let (session_id, start_key, end_key) = host::deserialize_session_range_param(range_in)?;
    let items = mudu_range(session_id, &start_key, &end_key)?;
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
        mudu_sys::env_var::temp_dir().join(format!("sys_interface_sync_{name}_{suffix}.db"))
    }

    fn with_adapter_test_db<F>(name: &str, f: F)
    where
        F: FnOnce(&std::path::Path),
    {
        let _guard = mudu_adapter::config::test_lock()
            .lock()
            .expect("test lock poisoned");
        let db_path = temp_db_path(name);
        mudu_adapter::syscall::set_db_path(&db_path);
        f(&db_path);
    }

    #[test]
    fn mudu_open_bytes_roundtrips_session_id() {
        with_adapter_test_db("open_bytes", |_db_path| {
            let output = mudu_open_bytes(&host::serialize_open_param()).unwrap();
            let session_id = host::deserialize_open_result(&output).unwrap();
            assert!(session_id > 0);
        });
    }

    #[test]
    fn mudu_close_bytes_rejects_missing_session() {
        with_adapter_test_db("close_bytes", |_db_path| {
            let oid = next_oid();
            let err = mudu_close_bytes(&host::serialize_close_param(oid)).unwrap_err();
            assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
        });
    }

    #[test]
    fn mudu_get_bytes_rejects_missing_session() {
        with_adapter_test_db("get_bytes", |_db_path| {
            let oid = next_oid();
            let err = mudu_get_bytes(&host::serialize_session_get_param(oid, b"k")).unwrap_err();
            assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
        });
    }

    #[test]
    fn mudu_put_bytes_rejects_missing_session() {
        with_adapter_test_db("put_bytes", |_db_path| {
            let oid = next_oid();
            let err =
                mudu_put_bytes(&host::serialize_session_put_param(oid, b"k", b"v")).unwrap_err();
            assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
        });
    }

    #[test]
    fn mudu_range_bytes_rejects_missing_session() {
        with_adapter_test_db("range_bytes", |_db_path| {
            let oid = next_oid();
            let err = mudu_range_bytes(&host::serialize_session_range_param(oid, b"a", b"z"))
                .unwrap_err();
            assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
        });
    }
}
