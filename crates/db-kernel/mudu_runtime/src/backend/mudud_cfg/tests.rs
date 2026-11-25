use super::io::{read_mudud_cfg, write_mudud_cfg};
use crate::backend::cfg_meta::ConfigMutability;
use crate::backend::mudud_cfg::MuduDBCfg;
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

/// A single-data-group Combined topology: no `control_plane`, one Raft group.
fn topology_cfg_toml() -> &'static str {
    r#"
version = 1
mpk_path = "/tmp/mpk"
db_path = "/tmp/data"
listen_ip = "127.0.0.1"
http_listen_port = 8300
http_worker_threads = 1
pg_listen_port = 5432
enable_async = true
tcp_listen_port = 9527

name = "g1-node-1"
cluster = "prod"
channel_listen_port = 29527
topology_version = 1

[data_plane]
rebalance_interval_ms = 0

[[data_plane.group]]
name = "grp-1"
worker_count = 4

[data_plane.group.replica.raft]

[[data_plane.group.replica.member]]
name = "g1-node-1"
host = "127.0.0.1"
channel_port = 29527
client_port = 5432
replication_port = 19527

[[data_plane.group.replica.member]]
name = "g1-node-2"
host = "127.0.0.1"
channel_port = 29528
client_port = 5433
replication_port = 19528
"#
}

fn try_parse_cfg(text: &str) -> RS<MuduDBCfg> {
    let path = mudu_sys::env_var::temp_dir().join(format!(
        "mudu/mudud_cfg_topology-{}",
        mudu_sys::random::uuid_v4()
    ));
    mudu_sys::fs::sync::write(&path, text).unwrap();
    read_mudud_cfg(path)
}

fn parse_cfg(text: &str) -> MuduDBCfg {
    try_parse_cfg(text).unwrap()
}

#[test]
fn test_topology_parse_and_defaults() {
    let cfg = parse_cfg(topology_cfg_toml());
    assert_eq!(cfg.version, 1);
    assert_eq!(cfg.name.as_deref(), Some("g1-node-1"));
    assert_eq!(cfg.cluster.as_deref(), Some("prod"));
    let data_plane = cfg.data_plane.as_ref().expect("data_plane parsed");
    assert_eq!(data_plane.group.len(), 1);
    let group = &data_plane.group[0];
    assert_eq!(group.name, "grp-1");
    assert_eq!(group.worker_count, 4);
    assert!(group.replica.raft.is_some());
    assert!(group.replica.stream.is_none());
    assert_eq!(group.replica.member.len(), 2);
    let raft = group.replica.raft.as_ref().unwrap();
    assert_eq!(raft.tick_ms, 50);
    assert_eq!(raft.election_timeout_ticks, 500);
    assert!(cfg.control_plane.is_none());
    assert!(cfg.validate_topology().is_ok());
}

#[test]
fn test_topology_absent_is_single_node() {
    let cfg = MuduDBCfg::default();
    assert!(cfg.data_plane.is_none());
    assert!(cfg.validate_topology().is_ok());
}

#[test]
fn test_topology_rejects_empty_members() {
    let text = topology_cfg_toml().replace(
        "[[data_plane.group.replica.member]]
name = \"g1-node-1\"
host = \"127.0.0.1\"
channel_port = 29527
client_port = 5432
replication_port = 19527

[[data_plane.group.replica.member]]
name = \"g1-node-2\"
host = \"127.0.0.1\"
channel_port = 29528
client_port = 5433
replication_port = 19528",
        "",
    );
    let cfg = parse_cfg(&text);
    assert!(cfg.validate_topology().is_err());
}

#[test]
fn test_topology_rejects_self_missing() {
    // The local node (top-level name) is not a member of any group.
    let text = topology_cfg_toml().replace(
        "name = \"g1-node-1\"\ncluster",
        "name = \"other-node\"\ncluster",
    );
    let cfg = parse_cfg(&text);
    assert!(cfg.validate_topology().is_err());
}

#[test]
fn test_topology_rejects_both_modes() {
    // Both raft and stream declared.
    let text = topology_cfg_toml().replace(
        "[data_plane.group.replica.raft]",
        "[data_plane.group.replica.raft]\n\n[data_plane.group.replica.stream]\nbootstrap_primary = \"g1-node-1\"\nsync = true",
    );
    let cfg = parse_cfg(&text);
    assert!(cfg.validate_topology().is_err());
}

#[test]
fn test_topology_rejects_mode_less_with_multiple_members() {
    // Neither mode declared, but two members remain: single replica needs one.
    let text = topology_cfg_toml().replace("[data_plane.group.replica.raft]\n", "");
    let cfg = parse_cfg(&text);
    assert!(cfg.validate_topology().is_err());
}

#[test]
fn test_topology_single_replica() {
    // Neither mode with exactly one member is a valid single-replica group.
    let text = topology_cfg_toml().replace(
        "[data_plane.group.replica.raft]\n\n[[data_plane.group.replica.member]]\nname = \"g1-node-1\"\nhost = \"127.0.0.1\"\nchannel_port = 29527\nclient_port = 5432\nreplication_port = 19527\n\n[[data_plane.group.replica.member]]\nname = \"g1-node-2\"\nhost = \"127.0.0.1\"\nchannel_port = 29528\nclient_port = 5433\nreplication_port = 19528",
        "[[data_plane.group.replica.member]]\nname = \"g1-node-1\"\nhost = \"127.0.0.1\"\nchannel_port = 29527\nclient_port = 5432\nreplication_port = 19527",
    );
    let cfg = parse_cfg(&text);
    let group = &cfg.data_plane.as_ref().unwrap().group[0];
    assert!(group.replica.raft.is_none());
    assert!(group.replica.stream.is_none());
    assert_eq!(group.replica.member.len(), 1);
    assert!(cfg.validate_topology().is_ok());
}

#[test]
fn test_topology_combined_requires_single_group() {
    // Two data groups without a control plane is not a valid Combined layout.
    let second_group = r#"
[[data_plane.group]]
name = "grp-2"
worker_count = 4

[data_plane.group.replica.raft]

[[data_plane.group.replica.member]]
name = "g2-node-1"
host = "127.0.0.1"
channel_port = 29529
client_port = 5434
replication_port = 19529
"#;
    let text = format!("{}{}", topology_cfg_toml(), second_group);
    let cfg = parse_cfg(&text);
    assert!(cfg.validate_topology().is_err());
}

#[test]
fn test_topology_dedicated_control_plane() {
    let control = r#"
[control_plane.replica.raft]

[[control_plane.replica.member]]
name = "ctl-1"
host = "127.0.0.1"
channel_port = 29530
client_port = 5434
"#;
    let text = format!("{}{}", topology_cfg_toml(), control);
    let cfg = parse_cfg(&text);
    assert!(cfg.control_plane.is_some());
    assert!(cfg.validate_topology().is_ok());
}

#[test]
fn test_topology_multi_group_dedicated_control_plane() {
    // Scale-out: a dedicated control plane plus two data groups. Without the
    // control plane this would be rejected (Combined needs exactly one group).
    let control = r#"
[control_plane.replica.raft]

[[control_plane.replica.member]]
name = "ctl-1"
host = "127.0.0.1"
channel_port = 29530
client_port = 5434
"#;
    let second_group = r#"
[[data_plane.group]]
name = "grp-2"
worker_count = 4

[data_plane.group.replica.raft]

[[data_plane.group.replica.member]]
name = "g2-node-1"
host = "127.0.0.1"
channel_port = 29529
client_port = 5435
replication_port = 19529
"#;
    let text = format!("{}{}{}", topology_cfg_toml(), control, second_group);
    let cfg = parse_cfg(&text);
    assert_eq!(cfg.data_plane.as_ref().unwrap().group.len(), 2);
    assert!(cfg.validate_topology().is_ok());
}

#[test]
fn test_topology_stream_parse_and_defaults() {
    let text = topology_cfg_toml().replace(
        "[data_plane.group.replica.raft]",
        "[data_plane.group.replica.stream]\nbootstrap_primary = \"g1-node-1\"\nsync = false",
    );
    let cfg = parse_cfg(&text);
    let group = &cfg.data_plane.as_ref().unwrap().group[0];
    assert!(group.replica.raft.is_none());
    let stream = group.replica.stream.as_ref().unwrap();
    assert_eq!(stream.bootstrap_primary, "g1-node-1");
    assert!(!stream.sync);
    assert_eq!(stream.sync_timeout_ms, 10000);
    assert!(cfg.validate_topology().is_ok());
}

#[test]
fn test_topology_stream_rejects_foreign_bootstrap_primary() {
    let text = topology_cfg_toml().replace(
        "[data_plane.group.replica.raft]",
        "[data_plane.group.replica.stream]\nbootstrap_primary = \"other-node\"\nsync = false",
    );
    let cfg = parse_cfg(&text);
    assert!(cfg.validate_topology().is_err());
}

#[test]
fn test_topology_stream_requires_keys() {
    // bootstrap_primary/sync are structurally required: a missing key fails
    // parsing.
    let base = topology_cfg_toml().replace(
        "[data_plane.group.replica.raft]",
        "[data_plane.group.replica.stream]",
    );
    assert!(try_parse_cfg(&base).is_err());
    let toml = base.replace(
        "[data_plane.group.replica.stream]",
        "[data_plane.group.replica.stream]\nbootstrap_primary = \"g1-node-1\"",
    );
    assert!(try_parse_cfg(&toml).is_err());
    let toml = base.replace(
        "[data_plane.group.replica.stream]",
        "[data_plane.group.replica.stream]\nsync = true",
    );
    assert!(try_parse_cfg(&toml).is_err());
}

#[test]
fn test_topology_rejects_port_collision() {
    let text =
        topology_cfg_toml().replace("channel_listen_port = 29527", "channel_listen_port = 9527");
    let cfg = parse_cfg(&text);
    assert!(cfg.validate_topology().is_err());
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

#[test]
fn queue_capacity_config_defaults_and_overrides() {
    let defaults = MuduDBCfg::default();
    assert_eq!(defaults.queue_max_msgs, 65536);
    assert_eq!(defaults.queue_max_bytes, 64 * 1024 * 1024);
    let mut text = toml::to_string(&defaults).unwrap();
    text = text
        .replace("queue_max_msgs = 65536", "queue_max_msgs = 2")
        .replace("queue_max_bytes = 67108864", "queue_max_bytes = 32");
    let custom: MuduDBCfg = toml::from_str(&text).unwrap();
    assert_eq!((custom.queue_max_msgs, custom.queue_max_bytes), (2, 32));
    let legacy = text
        .lines()
        .filter(|line| !line.starts_with("queue_max_"))
        .collect::<Vec<_>>()
        .join("\n");
    let legacy: MuduDBCfg = toml::from_str(&legacy).unwrap();
    assert_eq!(
        (legacy.queue_max_msgs, legacy.queue_max_bytes),
        (65536, 67108864)
    );
}
