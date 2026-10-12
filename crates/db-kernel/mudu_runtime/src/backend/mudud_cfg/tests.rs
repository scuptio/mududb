use super::io::{read_mudud_cfg, write_mudud_cfg};
use crate::backend::cfg_meta::ConfigMutability;
use crate::backend::mudud_cfg::{MuduDBCfg, ReplicationMode};
use mudu::common::result::RS;

#[test]
fn test_conf() {
    let cfg = MuduDBCfg::default();
    let path = mudu_sys::env_var::temp_dir().join("mudu/mudud.cfg");
    let r = write_mudud_cfg(path.clone(), &cfg);
    assert!(r.is_ok());
    let r = read_mudud_cfg(path.clone());
    assert!(r.is_ok());
    let conf1 = r.unwrap();
    assert_eq!(conf1, cfg);
}

#[test]
fn test_conf_with_comments_and_string_enums() {
    let path = mudu_sys::env_var::temp_dir().join("mudu/mudud_cfg_with_comments.cfg");
    if let Some(parent) = path.parent() {
        mudu_sys::fs::sync::create_dir_all(parent).unwrap();
    }
    mudu_sys::fs::sync::write(
        &path,
        r#"
# Example config with comments
mpk_path = "/tmp/mpk"
db_path = "/tmp/data"
listen_ip = "127.0.0.1"
http_listen_port = 8300
http_worker_threads = 1
pg_listen_port = 5432
enable_async = true

# "Legacy", "IOUring", or "Tokio"
server_mode = "IOUring"
tcp_listen_port = 9527
worker_threads = 0
io_uring_ring_entries = 1024
io_uring_accept_multishot = true
io_uring_recv_multishot = true
io_uring_enable_fixed_buffers = false
io_uring_enable_fixed_files = false

# "ConnectionId", "PlayerId", or "RemoteHash"
routing_mode = "ConnectionId"
"#,
    )
    .unwrap();

    let cfg = read_mudud_cfg(path).unwrap();
    assert_eq!(
        cfg.server_mode,
        crate::backend::mudud_cfg::ServerMode::IOUring
    );
    assert_eq!(
        cfg.routing_mode,
        crate::backend::mudud_cfg::RoutingMode::ConnectionId
    );
    assert_eq!(cfg.db_path, "/tmp/data");
    assert_eq!(cfg.http_worker_threads, 1);
}

#[test]
fn test_page_size_default() {
    let cfg = MuduDBCfg::default();
    assert_eq!(cfg.page_size, 4096);
}

#[test]
fn test_mutability_of_known_fields() {
    assert_eq!(
        MuduDBCfg::mutability_of("page_size"),
        ConfigMutability::Persistent
    );
    assert_eq!(
        MuduDBCfg::mutability_of("db_path"),
        ConfigMutability::Persistent
    );
    assert_eq!(
        MuduDBCfg::mutability_of("worker_threads"),
        ConfigMutability::RestartRequired
    );
    assert_eq!(
        MuduDBCfg::mutability_of("tcp_multi_port"),
        ConfigMutability::Runtime
    );
}

fn replication_cfg_toml() -> &'static str {
    r#"
mpk_path = "/tmp/mpk"
db_path = "/tmp/data"
listen_ip = "127.0.0.1"
http_listen_port = 8300
http_worker_threads = 1
pg_listen_port = 5432
enable_async = true
tcp_listen_port = 9527

[replication]
node_id = 1
listen_port = 19527

[replication.raft]

[[replication.peers]]
node_id = 1
host = "127.0.0.1"
port = 19527

[[replication.peers]]
node_id = 2
host = "127.0.0.1"
port = 19528
"#
}

#[test]
fn test_replication_parse_and_defaults() {
    let path = mudu_sys::env_var::temp_dir().join("mudu/mudud_cfg_replication.cfg");
    if let Some(parent) = path.parent() {
        mudu_sys::fs::sync::create_dir_all(parent).unwrap();
    }
    mudu_sys::fs::sync::write(&path, replication_cfg_toml()).unwrap();
    let cfg = read_mudud_cfg(path).unwrap();
    let replication = cfg
        .replication
        .as_ref()
        .expect("replication section parsed");
    assert_eq!(replication.mode().unwrap(), ReplicationMode::Raft);
    assert_eq!(replication.node_id, 1);
    assert_eq!(replication.listen_port, 19527);
    let raft = replication.raft.as_ref().expect("raft subsection parsed");
    assert_eq!(raft.raft_tick_ms, 50);
    assert_eq!(raft.election_timeout_ticks, 500);
    assert_eq!(replication.repl_flow_window_segments, 64);
    assert_eq!(replication.snapshot_wal_threshold_bytes, 1024 * 1024 * 1024);
    assert_eq!(replication.gc_check_interval_secs, 60);
    assert!(replication.stream.is_none());
    assert_eq!(replication.peers.len(), 2);
    // a two-peer raft list is valid (bootstrap shape of a joining node)
    assert!(cfg.validate_replication().is_ok());
}

#[test]
fn test_replication_absent_is_single_node() {
    let cfg = MuduDBCfg::default();
    assert!(cfg.replication.is_none());
    assert!(cfg.validate_replication().is_ok());
}

#[test]
fn test_replication_validate_rejects_bad_peers() {
    let parse = |peers: &str| {
        let text = replication_cfg_toml().replace(
            "[[replication.peers]]
node_id = 1
host = \"127.0.0.1\"
port = 19527

[[replication.peers]]
node_id = 2
host = \"127.0.0.1\"
port = 19528",
            peers,
        );
        let path = mudu_sys::env_var::temp_dir().join("mudu/mudud_cfg_replication_bad.cfg");
        mudu_sys::fs::sync::write(&path, text).unwrap();
        read_mudud_cfg(path).unwrap()
    };
    // empty peers
    let cfg = parse("");
    assert!(cfg.validate_replication().is_err());
    // duplicate node_id
    let cfg = parse(
        "[[replication.peers]]
node_id = 1
host = \"127.0.0.1\"
port = 19527

[[replication.peers]]
node_id = 1
host = \"127.0.0.1\"
port = 19528",
    );
    assert!(cfg.validate_replication().is_err());
    // self missing
    let cfg = parse(
        "[[replication.peers]]
node_id = 2
host = \"127.0.0.1\"
port = 19528",
    );
    assert!(cfg.validate_replication().is_err());
}

#[test]
fn test_replication_validate_rejects_port_collision() {
    let text = replication_cfg_toml().replace("listen_port = 19527", "listen_port = 9527");
    let path = mudu_sys::env_var::temp_dir().join("mudu/mudud_cfg_replication_collision.cfg");
    mudu_sys::fs::sync::write(&path, text).unwrap();
    let cfg = read_mudud_cfg(path).unwrap();
    assert!(cfg.validate_replication().is_err());
}

#[test]
fn test_load_or_create_node_id() {
    let dir =
        mudu_sys::env_var::temp_dir().join(format!("mudu/node-id-{}", mudu_sys::random::uuid_v4()));
    let created = crate::backend::mudud_cfg::load_or_create_node_id(&dir, 7).unwrap();
    assert_eq!(created, 7);
    let reloaded = crate::backend::mudud_cfg::load_or_create_node_id(&dir, 7).unwrap();
    assert_eq!(reloaded, 7);
    assert!(crate::backend::mudud_cfg::load_or_create_node_id(&dir, 8).is_err());
}

fn try_parse_replication_cfg(text: &str) -> RS<MuduDBCfg> {
    let path = mudu_sys::env_var::temp_dir().join(format!(
        "mudu/mudud_cfg_mode-{}",
        mudu_sys::random::uuid_v4()
    ));
    mudu_sys::fs::sync::write(&path, text).unwrap();
    read_mudud_cfg(path)
}

fn parse_replication_cfg(text: &str) -> MuduDBCfg {
    try_parse_replication_cfg(text).unwrap()
}

#[test]
fn test_replication_subsection_is_required() {
    // both subsections absent: parses fine, but the mode selection is
    // reported by mode() and by validation
    let toml = replication_cfg_toml().replace("[replication.raft]\n\n", "");
    let cfg = parse_replication_cfg(&toml);
    let replication = cfg.replication.as_ref().unwrap();
    let err = replication.mode().unwrap_err();
    assert!(
        err.to_string().contains("requires exactly one of"),
        "unexpected error: {err}"
    );
    assert!(cfg.validate_replication().is_err());
}

#[test]
fn test_replication_subsections_mutually_exclusive() {
    let toml = replication_cfg_toml().replace(
        "[replication.raft]",
        "[replication.raft]\n\n[replication.stream]\nprimary = true\nsync = true",
    );
    let cfg = parse_replication_cfg(&toml);
    let replication = cfg.replication.as_ref().unwrap();
    let err = replication.mode().unwrap_err();
    assert!(
        err.to_string().contains("mutually exclusive"),
        "unexpected error: {err}"
    );
    assert!(cfg.validate_replication().is_err());
}

#[test]
fn test_replication_raft_two_peers_allowed_for_bootstrap() {
    // Two peers is the sanctioned bootstrap shape of a joining node
    // (itself + one existing member); steady-state two-node clusters are
    // discouraged but not rejected.
    let cfg = parse_replication_cfg(replication_cfg_toml());
    assert!(cfg.validate_replication().is_ok());
}

#[test]
fn test_replication_snapshot_gc_knobs_validated() {
    let toml = replication_cfg_toml().replace(
        "listen_port = 19527",
        "listen_port = 19527\nsnapshot_wal_threshold_bytes = 0",
    );
    let cfg = parse_replication_cfg(&toml);
    let err = cfg.validate_replication().unwrap_err();
    assert!(
        err.to_string().contains("snapshot_wal_threshold_bytes"),
        "unexpected error: {err}"
    );
    let toml = replication_cfg_toml().replace(
        "listen_port = 19527",
        "listen_port = 19527\ngc_check_interval_secs = 0",
    );
    let cfg = parse_replication_cfg(&toml);
    let err = cfg.validate_replication().unwrap_err();
    assert!(
        err.to_string().contains("gc_check_interval_secs"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_replication_stream_requires_primary_and_sync_keys() {
    // primary/sync are structurally required: a missing key fails parsing
    let base = replication_cfg_toml().replace("[replication.raft]", "[replication.stream]");
    assert!(try_parse_replication_cfg(&base).is_err());
    let toml = base.replace(
        "[replication.stream]",
        "[replication.stream]\nprimary = true",
    );
    assert!(try_parse_replication_cfg(&toml).is_err());
    let toml = base.replace("[replication.stream]", "[replication.stream]\nsync = true");
    assert!(try_parse_replication_cfg(&toml).is_err());
}

#[test]
fn test_replication_stream_parse_and_defaults() {
    let toml = replication_cfg_toml().replace(
        "[replication.raft]",
        "[replication.stream]\nprimary = true\nsync = false",
    );
    let cfg = parse_replication_cfg(&toml);
    cfg.validate_replication().unwrap();
    let replication = cfg.replication.as_ref().unwrap();
    assert_eq!(replication.mode().unwrap(), ReplicationMode::Stream);
    let stream = replication.stream.as_ref().unwrap();
    assert!(stream.primary);
    assert!(!stream.sync);
    assert_eq!(stream.sync_timeout_ms, 10000);
    assert!(replication.raft.is_none());
}
