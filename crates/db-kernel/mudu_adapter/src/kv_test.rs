//! Unit tests for the key-value backend adapter.

#![allow(missing_docs)]
// Tests assert expected failures with `panic!`; allowed because this is test-only code.
#![allow(clippy::panic)]

use crate::config;
use crate::kv;
use crate::sqlite;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu_sys::env_var;
use std::path::PathBuf;

fn temp_db_path(label: &str) -> PathBuf {
    let mut path = env_var::temp_dir();
    path.push(format!(
        "mududb_kv_test_{}_{}.db",
        std::process::id(),
        label
    ));
    path
}

fn with_sqlite_db<T>(label: &str, f: impl FnOnce() -> RS<T>) -> RS<T> {
    let _guard = config::test_lock().lock()?;
    let path = temp_db_path(label);
    config::set_db_path(&path);
    let result = f();
    config::reset_db_path_override_for_test();
    let _ = mudu_sys::fs::sync::remove_file(&path);
    result
}

#[test]
fn kv_get_put_and_range_roundtrip() -> RS<()> {
    with_sqlite_db("sync", || {
        let sid = sqlite::mudu_open()?;

        assert!(kv::get(sid, b"missing")?.is_none());

        kv::put(sid, b"k1", b"v1")?;
        kv::put(sid, b"k2", b"v2")?;
        kv::put(sid, b"k3", b"v3")?;

        assert_eq!(kv::get(sid, b"k1")?, Some(b"v1".to_vec()));
        assert_eq!(kv::get(sid, b"k2")?, Some(b"v2".to_vec()));

        let all = kv::range(sid, b"", b"")?;
        assert_eq!(all.len(), 3);

        let subset = kv::range(sid, b"k1", b"k3")?;
        assert_eq!(subset.len(), 2);
        assert_eq!(subset[0].0, b"k1".to_vec());
        assert_eq!(subset[1].0, b"k2".to_vec());

        kv::put(sid, b"k1", b"v1-updated")?;
        assert_eq!(kv::get(sid, b"k1")?, Some(b"v1-updated".to_vec()));

        sqlite::mudu_close(sid)?;
        Ok(())
    })
}

#[test]
fn kv_async_get_put_and_range_roundtrip() -> RS<()> {
    with_sqlite_db("async", || {
        let sid = sqlite::mudu_open()?;

        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            kv::ensure_session_exists_async(sid).await?;

            assert!(kv::get_async(sid, b"missing").await?.is_none());

            kv::put_async(sid, b"a1", b"b1").await?;
            kv::put_async(sid, b"a2", b"b2").await?;

            assert_eq!(kv::get_async(sid, b"a1").await?, Some(b"b1".to_vec()));

            let range = kv::range_async(sid, b"a1", b"a3").await?;
            assert_eq!(range.len(), 2);
            Ok::<(), mudu::error::MuduError>(())
        })??;

        sqlite::mudu_close(sid)?;
        Ok(())
    })
}

#[test]
fn ensure_session_exists_errors_when_session_missing() -> RS<()> {
    with_sqlite_db("missing_session", || {
        let err = match kv::ensure_session_exists(12345) {
            Ok(_) => panic!("expected entity not found error"),
            Err(e) => e,
        };
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
        Ok(())
    })
}

#[test]
fn ensure_session_exists_errors_for_non_sqlite_driver() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();

    let prev = env_var::var("MUDU_CONNECTION");
    env_var::set_var("MUDU_CONNECTION", "postgres://localhost/test");

    let err = match kv::ensure_session_exists(1) {
        Ok(_) => panic!("expected not implemented error"),
        Err(e) => e,
    };
    assert_eq!(err.ec(), ErrorCode::NotImplemented);

    match prev {
        Some(prev) => env_var::set_var("MUDU_CONNECTION", &prev),
        None => env_var::remove_var("MUDU_CONNECTION"),
    }
    Ok(())
}
