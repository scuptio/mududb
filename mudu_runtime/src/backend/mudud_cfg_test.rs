#![allow(clippy::unwrap_used)]

use super::{
    MuduDBCfg, RoutingMode, ServerMode, init_mudud_cfg, load_mudud_cfg, load_mudud_cfg_with_local,
};
use std::time::UNIX_EPOCH;

fn temp_cfg_name() -> String {
    let nanos = mudu_sys::time::system_time_now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("mudud-{nanos}.cfg")
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
fn load_explicit_path_missing_returns_error() {
    let current_dir_cfg = mudu_sys::env_var::current_dir()
        .unwrap()
        .join(temp_cfg_name());

    if current_dir_cfg.exists() {
        let _ = mudu_sys::fs::sync::remove_file(&current_dir_cfg);
    }

    let result = load_mudud_cfg(Some(current_dir_cfg.to_string_lossy().to_string()));
    assert!(result.is_err());
}

#[test]
fn load_searches_home_mududb_config_when_local_missing() {
    let home = mudu_sys::env_var::temp_dir().join(temp_cfg_name());
    let global_cfg = home.join(".mududb").join("mudud.cfg");
    let missing_local_cfg = mudu_sys::env_var::temp_dir().join(temp_cfg_name());

    if global_cfg.exists() {
        let _ = mudu_sys::fs::sync::remove_file(&global_cfg);
    }
    if missing_local_cfg.exists() {
        let _ = mudu_sys::fs::sync::remove_file(&missing_local_cfg);
    }
    if let Some(parent) = global_cfg.parent() {
        let _ = mudu_sys::fs::sync::create_dir_all(parent);
    }

    mudu_sys::fs::sync::write(
        &global_cfg,
        br#"
mpk_path = "/global/mpk"
db_path = "/global/data"
listen_ip = "127.0.0.1"
http_listen_port = 18300
pg_listen_port = 15432
server_mode = "IOUring"
routing_mode = "RemoteHash"
enable_async = false
"#,
    )
    .unwrap();

    let cfg = load_mudud_cfg_with_local(missing_local_cfg, Some(home), true).unwrap();
    assert_eq!(cfg.mpk_path, "/global/mpk");
    assert_eq!(cfg.db_path, "/global/data");
    assert_eq!(cfg.server_mode, ServerMode::IOUring);
    assert_eq!(cfg.routing_mode, RoutingMode::RemoteHash);

    let _ = mudu_sys::fs::sync::remove_file(&global_cfg);
}

#[test]
fn load_prefers_local_config_over_global() {
    let home = mudu_sys::env_var::temp_dir().join(temp_cfg_name());
    let global_cfg = home.join(".mududb").join("mudud.cfg");
    let local_cfg = mudu_sys::env_var::temp_dir().join(temp_cfg_name());

    for path in [&global_cfg, &local_cfg] {
        if path.exists() {
            let _ = mudu_sys::fs::sync::remove_file(path);
        }
        if let Some(parent) = path.parent() {
            let _ = mudu_sys::fs::sync::create_dir_all(parent);
        }
    }

    mudu_sys::fs::sync::write(
        &global_cfg,
        br#"
mpk_path = "/global/mpk"
db_path = "/global/data"
listen_ip = "127.0.0.1"
http_listen_port = 18300
pg_listen_port = 15432
server_mode = "IOUring"
routing_mode = "RemoteHash"
enable_async = false
"#,
    )
    .unwrap();

    mudu_sys::fs::sync::write(
        &local_cfg,
        br#"
mpk_path = "/local/mpk"
db_path = "/local/data"
listen_ip = "127.0.0.1"
http_listen_port = 18301
pg_listen_port = 15433
server_mode = "Tokio"
routing_mode = "ConnectionId"
enable_async = true
"#,
    )
    .unwrap();

    let cfg = load_mudud_cfg_with_local(local_cfg.clone(), Some(home), true).unwrap();
    assert_eq!(cfg.mpk_path, "/local/mpk");
    assert_eq!(cfg.db_path, "/local/data");
    assert_eq!(cfg.server_mode, ServerMode::Tokio);
    assert_eq!(cfg.routing_mode, RoutingMode::ConnectionId);

    let _ = mudu_sys::fs::sync::remove_file(&global_cfg);
    let _ = mudu_sys::fs::sync::remove_file(&local_cfg);
}

#[test]
fn load_returns_error_when_no_config_anywhere() {
    let home = mudu_sys::env_var::temp_dir().join(temp_cfg_name());
    let global_cfg = home.join(".mududb").join("mudud.cfg");
    let missing_local_cfg = mudu_sys::env_var::temp_dir().join(temp_cfg_name());

    if global_cfg.exists() {
        let _ = mudu_sys::fs::sync::remove_file(&global_cfg);
    }
    if missing_local_cfg.exists() {
        let _ = mudu_sys::fs::sync::remove_file(&missing_local_cfg);
    }

    let result = load_mudud_cfg_with_local(missing_local_cfg, Some(home), true);
    assert!(result.is_err());
}

#[test]
fn load_uses_existing_current_directory_config() {
    let current_dir_cfg = mudu_sys::env_var::current_dir()
        .unwrap()
        .join(temp_cfg_name());

    mudu_sys::fs::sync::write(
        &current_dir_cfg,
        br#"
mpk_path = "/tmp/test/mpk"
db_path = "/tmp/test/data"
listen_ip = "127.0.0.1"
http_listen_port = 18300
pg_listen_port = 15432
server_mode = "Tokio"
routing_mode = "RemoteHash"
enable_async = false
"#,
    )
    .unwrap();

    let cfg = load_mudud_cfg(Some(current_dir_cfg.to_string_lossy().to_string())).unwrap();
    assert_eq!(cfg.mpk_path, "/tmp/test/mpk");
    assert_eq!(cfg.db_path, "/tmp/test/data");
    assert_eq!(cfg.server_mode, ServerMode::Tokio);
    assert_eq!(cfg.routing_mode, RoutingMode::RemoteHash);

    let _ = mudu_sys::fs::sync::remove_file(&current_dir_cfg);
}

#[test]
fn init_cfg_writes_to_current_directory() {
    let current_dir_cfg = mudu_sys::env_var::current_dir().unwrap().join("mudud.cfg");
    if current_dir_cfg.exists() {
        let _ = mudu_sys::fs::sync::remove_file(&current_dir_cfg);
    }

    init_mudud_cfg().unwrap();
    assert!(mudu_sys::fs::sync::sync_path_exists(&current_dir_cfg));

    let content = mudu_sys::fs::sync::read_to_string(&current_dir_cfg).unwrap();
    assert!(content.contains("# Directory containing .mpk application packages."));
    assert!(content.contains("mpk_path = \"./mpk\""));
    assert!(content.contains("db_path = \"./data\""));
    assert!(content.contains("server_mode = \"Tokio\""));

    let cfg = load_mudud_cfg(Some(current_dir_cfg.to_string_lossy().to_string())).unwrap();
    assert_eq!(cfg.mpk_path, "./mpk");
    assert_eq!(cfg.db_path, "./data");
    assert_eq!(cfg.server_mode, ServerMode::Tokio);

    let _ = mudu_sys::fs::sync::remove_file(&current_dir_cfg);
}
