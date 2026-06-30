//! Tests for `mudud` configuration loading and the serve/stop path.
#![allow(missing_docs)]

use crate::{Args, serve_with_stop, serve_with_stop_and_runner, spawn_signal_listener};
use mudu_sys::fs::sync::sync_write;
use mudu_utils::notifier::notify_wait;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

fn unique_path(prefix: &str) -> String {
    let uuid = mudu_sys::random::Uuid::new_v4();
    mudu_sys::env_var::temp_dir()
        .join(format!("{}_{}.toml", prefix, uuid))
        .to_string_lossy()
        .into_owned()
}

#[test]
fn serve_with_stop_creates_default_config_when_missing() -> Result<(), Box<dyn std::error::Error>> {
    let cfg_path = unique_path("missing_mududb_cfg");
    let args = Args {
        cfg_path: Some(cfg_path.clone()),
    };
    let (_notifier, stop_waiter) = notify_wait();

    let ran = Arc::new(AtomicBool::new(false));
    let ran_clone = ran.clone();

    let result = serve_with_stop_and_runner(args, stop_waiter, move |cfg, _stop| {
        assert!(!cfg.db_path.is_empty());
        assert!(!cfg.mpk_path.is_empty());
        ran_clone.store(true, Ordering::SeqCst);
        Ok(())
    });

    assert!(result.is_ok());
    assert!(ran.load(Ordering::SeqCst));
    // The missing config path should have been written with defaults.
    assert!(mudu_sys::fs::sync::sync_path_exists(&cfg_path));
    Ok(())
}

#[test]
fn serve_with_stop_uses_existing_config() -> Result<(), Box<dyn std::error::Error>> {
    let cfg_path = unique_path("existing_mududb_cfg");
    sync_write(
        &cfg_path,
        br#"
mpk_path = "/tmp/test/mpk"
data_path = "/tmp/test/data"
listen_ip = "127.0.0.1"
http_listen_port = 18300
pg_listen_port = 15432
enable_async = false
"#,
    )?;

    let args = Args {
        cfg_path: Some(cfg_path.clone()),
    };
    let (_notifier, stop_waiter) = notify_wait();

    let result = serve_with_stop_and_runner(args, stop_waiter, |cfg, _stop| {
        assert_eq!(cfg.db_path, "/tmp/test/data");
        assert_eq!(cfg.mpk_path, "/tmp/test/mpk");
        assert_eq!(cfg.http_listen_port, 18300);
        assert_eq!(cfg.pg_listen_port, 15432);
        Ok(())
    });

    assert!(result.is_ok());
    Ok(())
}

#[test]
fn serve_with_stop_propagates_runner_error() -> Result<(), Box<dyn std::error::Error>> {
    let cfg_path = unique_path("runner_error_cfg");
    let args = Args {
        cfg_path: Some(cfg_path.clone()),
    };
    let (_notifier, stop_waiter) = notify_wait();

    let result = serve_with_stop_and_runner(args, stop_waiter, |_cfg, _stop| {
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::Internal,
            "mock runner failure"
        ))
    });

    assert!(result.is_err());
    Ok(())
}

#[test]
fn serve_with_stop_propagates_config_decode_error() -> Result<(), Box<dyn std::error::Error>> {
    let cfg_path = unique_path("invalid_mududb_cfg");
    sync_write(&cfg_path, "this is not valid toml")?;
    let args = Args {
        cfg_path: Some(cfg_path.clone()),
    };
    let (_notifier, stop_waiter) = notify_wait();

    let result = serve_with_stop(args, stop_waiter);
    assert!(result.is_err());
    Ok(())
}

// Miri does not support sigaction, so signal handling cannot be exercised.
#[cfg_attr(miri, ignore)]
#[test]
fn spawn_signal_listener_stops_on_notifier() -> Result<(), Box<dyn std::error::Error>> {
    let (notifier, _waiter) = notify_wait();
    let handle = spawn_signal_listener(notifier.clone())?;

    // Give the listener a moment to register its signal handlers.
    mudu_sys::task::sync::sleep_blocking(Duration::from_millis(100));
    notifier.notify_all();

    assert!(handle.join().is_ok());
    Ok(())
}
