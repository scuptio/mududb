#![allow(clippy::unwrap_used)]

use super::{MuduDBCfg, ServerMode, load_mududb_cfg};
use std::time::UNIX_EPOCH;

fn temp_home() -> std::path::PathBuf {
    let nanos = mudu_sys::time::system_time_now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    mudu_sys::env_var::temp_dir().join(format!("mududb-cfg-home-{nanos}"))
}

#[test]
fn display_contains_all_fields() {
    let cfg = MuduDBCfg::default();
    let text = format!("{}", cfg);
    assert!(text.contains("MuduDB Setting:"));
    assert!(text.contains(&format!("Package path: {}", cfg.mpk_path)));
    assert!(text.contains(&format!("Data path: {}", cfg.db_path)));
    assert!(text.contains(&format!("Listen IP address: {}", cfg.listen_ip)));
    assert!(text.contains(&format!("HTTP Listening port: {}", cfg.http_listen_port)));
    assert!(text.contains(&format!("Component target: {:?}", cfg.component_target())));
    assert!(text.contains(&format!("Server mode: {:?}", cfg.server_mode)));
    assert!(text.contains(&format!("page size: {}", cfg.page_size)));
}

#[test]
fn uses_mududb_kernel_matches_server_mode() {
    let mut cfg = MuduDBCfg {
        server_mode: ServerMode::Legacy,
        ..Default::default()
    };
    assert!(!cfg.uses_mududb_kernel());

    cfg.server_mode = ServerMode::IOUring;
    assert!(cfg.uses_mududb_kernel());

    cfg.server_mode = ServerMode::Tokio;
    assert!(cfg.uses_mududb_kernel());
}

#[test]
fn effective_worker_threads_uses_config_or_parallelism() {
    let mut cfg = MuduDBCfg {
        worker_threads: 4,
        ..Default::default()
    };
    assert_eq!(cfg.effective_worker_threads(), 4);

    cfg.worker_threads = 0;
    let expected = std::thread::available_parallelism()
        .map(|v| v.get())
        .unwrap_or(1);
    assert_eq!(cfg.effective_worker_threads(), expected);
}

#[test]
fn load_from_default_home_path_creates_default_config() {
    let home = temp_home();
    mudu_sys::fs::sync::create_dir_all(&home).unwrap();
    let prev = mudu_sys::env_var::var("HOME");
    mudu_sys::env_var::set_var("HOME", home.to_str().unwrap());

    let cfg = load_mududb_cfg(None).unwrap();
    assert_eq!(cfg, MuduDBCfg::default());

    let expected_path = home.join(".mududb/mududb_cfg.toml");
    assert!(mudu_sys::fs::sync::sync_path_exists(&expected_path));

    match prev {
        Some(prev) => mudu_sys::env_var::set_var("HOME", &prev),
        None => mudu_sys::env_var::remove_var("HOME"),
    }
    let _ = mudu_sys::fs::sync::remove_file(&expected_path);
    let _ = mudu_sys::fs::sync::remove_dir_all(home.join(".mududb"));
    let _ = mudu_sys::fs::sync::remove_dir_all(home);
}
