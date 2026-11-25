# MuduDB Server Configuration Contract v1

## Scope

This document specifies the `mudud.cfg` server configuration file format. The file controls server listen ports, execution mode, runtime paths, and io_uring behavior.

## Version history

| Version | Date | Summary |
|---------|------|---------|
| 1 | 2026-6-25 | Initial TOML configuration. No explicit `version` field; compatibility is implicit through serde defaults and aliases. |
| 1 | 2026-10-12 | Documented existing backward-compatible fields (`wal_flush_max_wait_us`, `wal_sync_mode`, `wal_sync_interval_ms`, `[invocation_limits]`, `[replication]`, `[backup]`); corrected the documented `server_mode` default to `"Tokio"`. No format change. |
| 1 | 2026-11-20 | Cluster topology flattened to top-level `control_plane`/`data_plane`; nodes, data groups and the cluster are identified by unique `name`, with internal ids derived as u64 via FNV-1a 64; added an explicit `version = 1`. Breaking change, no compatibility. |

## File location

The server loads the configuration from the first file that exists, in this order:

1. The path provided by `--cfg /path/to/mudud.cfg` (or `-c /path/to/mudud.cfg`), if given.
2. `./mudud.cfg` in the current working directory.
3. `~/.mududb/mudud.cfg` in the user's home directory.

If none of these files exist, the server returns a `NotFound` error. Use `mudud init-cfg` to create a default `./mudud.cfg` before starting the server.

## Configuration fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `mpk_path` | string | `"./mpk"` | Path to the application package directory. |
| `db_path` | string | `"./data"` | Path to the database directory. |
| `listen_ip` | string | `127.0.0.1` | IP address to listen on. |
| `http_listen_port` | u16 | `8300` | HTTP management API port. |
| `http_worker_threads` | usize | `1` | HTTP worker thread count. |
| `pg_listen_port` | u16 | `5432` | PostgreSQL wire protocol port. |
| `component_target` | string | `p2` | Wasm component ABI target. Allowed: `p2`, `p3`. |
| `enable_async` | boolean | `true` | Enable the WASI component runtime. |
| `server_mode` | string | `"Tokio"` | `"Legacy"`, `"IOUring"`, or `"Tokio"`. |
| `tcp_listen_port` | u16 | `9527` | TCP framed protocol port. |
| `tcp_multi_port` | boolean | `false` | One TCP listener per worker. |
| `worker_threads` | usize | `0` | Worker thread count. `0` means use available parallelism. |
| `io_uring_ring_entries` | u32 | `1024` | io_uring completion queue depth. |
| `io_uring_accept_multishot` | boolean | `true` | Enable io_uring accept multishot. |
| `io_uring_recv_multishot` | boolean | `true` | Enable io_uring recv multishot. |
| `io_uring_enable_fixed_buffers` | boolean | `false` | Enable io_uring fixed buffers. |
| `io_uring_enable_fixed_files` | boolean | `false` | Enable io_uring fixed files. |
| `routing_mode` | string | `"ConnectionId"` | `"ConnectionId"`, `"PlayerId"`, or `"RemoteHash"`. |
| `log_chunk_size` | u64 | `64 * 1024 * 1024` | io_uring log chunk size in bytes. |
| `queue_max_msgs` | u64 | `65536` | Positive maximum retained messages per queue. |
| `queue_max_bytes` | u64 | `67108864` | Positive encoded payload byte cap; excludes system columns and storage overhead. |
| `page_size` | usize | `4096` | Database page size in bytes. This is a persistent setting; changing it for an existing database requires migration or re-initialization. |
| `wal_flush_max_wait_us` | u64 | `200` | Group-commit batching window in microseconds: how long a queued WAL commit batch may age before the flush driver flushes it. |
| `wal_sync_mode` | string | `"commit"` | WAL durability mode: `"commit"` fsyncs every flush round before the commit is acknowledged; `"periodic"` acknowledges once the WAL write lands in the page cache and fsyncs at most once per `wal_sync_interval_ms`. |
| `wal_sync_interval_ms` | u64 | `10` | WAL fsync interval in milliseconds for `wal_sync_mode = "periodic"`. A power loss may lose acknowledged commits from the last interval. |
| `version` | u32 | `1` | Configuration-format version; unknown versions are an error, not a silent downgrade. |
| `name` | string | (absent) | This node's unique name (cluster mode). |
| `cluster` | string | (absent) | Cluster name, identical across all nodes (cluster mode). |
| `channel_listen_port` | u16 | (absent) | Local inter-node channel listener (cluster mode). |
| `topology_version` | u64 | (absent) | Runtime topology revision/fencing value (cluster mode). |
| `epoch_sparse` | boolean | `false` | Enables the sparse epoch protocol (P4): the control plane advances the stable/current epoch in memory within a committed reserved range instead of committing an `Advance` record per tick. Off by default while the protocol rolls out. |

## Optional sections

### `[invocation_limits]`

Procedure budgets applied to every invocation; changes apply to newly constructed runtimes. Missing keys use the defaults below. Unknown keys are rejected (`deny_unknown_fields`).

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `memory_bytes` | usize | `128 * 1024 * 1024` | Total linear-memory bytes across all memories in one invocation. |
| `fuel` | u64 | `1000000000` | Wasm instruction fuel, including component initialization. |
| `timeout_ms` | u64 | `30000` | Wall-clock execution budget, including transaction setup. |
| `max_input_bytes` | usize | `8 * 1024 * 1024` | Maximum encoded procedure or host-call input size. |
| `max_output_bytes` | usize | `8 * 1024 * 1024` | Maximum encoded procedure or host-call output size. |
| `max_host_calls` | u64 | `10000` | Maximum host calls, including resource-opening calls. |
| `max_resources` | usize | `256` | Maximum simultaneously open worker sessions and file handles. |
| `table_elements` | usize | `100000` | Maximum total table elements in one invocation. |
| `max_concurrent` | usize | `4` | Process-wide maximum admitted procedure calls. |
| `max_concurrent_per_app` | usize | `2` | Maximum admitted calls sharing an application name. |
| `total_memory_bytes` | usize | `512 * 1024 * 1024` | Process-wide reserved guest linear-memory budget. |

### Cluster topology: `[control_plane]` and `[data_plane]`

> Current startup boundaries: data-group Raft knobs are wired, but dedicated control-group knobs do not reach `ControlPlaneNode::start`, which still constructs `RaftCfg::new(..., 25, 4, ...)`. Actual data Worker count comes from top-level `worker_threads` (0 means machine parallelism); set it to match the group's `worker_count`. Standalone shares control/data consensus only with an existing data Raft instance; stream or mode-less data still gets a separate control Raft. Cluster configuration currently uses Tokio; IOUring rejects `data_plane`. See [startup boundaries](../cluster_configuration.md).

Cluster mode consists of the top-level `name`, `cluster`, `channel_listen_port`, `topology_version` plus the `[control_plane]`/`[data_plane]` sections. When these are absent the server runs as a single node with no cluster components started.

Nodes, data groups and the cluster are identified by a unique `name` (string); the configuration never carries a numeric id. The internal numeric ids are derived deterministically by the FNV-1a 64-bit hash (offset basis `0xcbf29ce484222325`, prime `0x100000001b3`, no salt/seed), all u64:

| Internal id | Derivation |
|---|---|
| `node_id` | `fnv1a64(name)` over the UTF-8 bytes of the node/member name |
| `group_id` | `fnv1a64(group.name)` over the UTF-8 bytes of the group name |
| `cluster_id` | `fnv1a64(cluster)` over the UTF-8 bytes of the cluster name |

`[control_plane]` (optional; omitted selects Standalone, which requires exactly one data group):

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `replica.raft` | table | (required) | The dedicated control group's replication mode (Raft only). |
| `replica.member` | array of tables | (required) | `[[control_plane.replica.member]]` control bootstrap members. |

`[data_plane]`:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `group` | array of tables | (required) | `[[data_plane.group]]` data-group list. |
| `placements` | array of tables | `[]` | `[[data_plane.placements]]` static placement records. |
| `rebalance_interval_ms` | u64 | `0` | Rebalance interval; 0 disables scheduling. |

`[[data_plane.group]]` fields:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | string | (required) | Unique data-group name. |
| `worker_count` | u32 | (required) | Business Workers per member of this data group. |
| `replica.raft` | table | (at most one) | Selects Raft mode; mutually exclusive with `replica.stream`. |
| `replica.stream` | table | (at most one) | Selects stream mode; mutually exclusive with `replica.raft`. |
| `replica.member` | array of tables | (required) | `[[data_plane.group.replica.member]]` bootstrap members. |

When both `replica.raft` and `replica.stream` are omitted, the data group is a
single replica (no replication mechanism) and `replica.member` must hold exactly
one member; declaring both is an error.

`[data_plane.group.replica.raft]` fields:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `tick_ms` | u64 | `50` | Raft state machine tick interval in milliseconds. |
| `election_timeout_ticks` | u64 | `500` | Raft election timeout in ticks. |

`[data_plane.group.replica.stream]` fields:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `bootstrap_primary` | string | (required) | Name of the initial primary member; first boot only. |
| `sync` | boolean | (required) | Whether the primary waits for at least one StreamFollower's durable ack before answering a commit. |
| `sync_timeout_ms` | u64 | `10000` | Synchronous replication wait timeout in milliseconds. |

`[[control_plane.replica.member]]` fields:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | string | (required) | Unique name of the control member. |
| `host` | string | (required) | Hostname or IP the control member's channel listens on. |
| `channel_port` | u16 | (required) | Control member channel port. |
| `client_port` | u16 | (required) | Control member client port. |

`[[data_plane.group.replica.member]]` fields:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | string | (required) | Unique name of the data member. |
| `host` | string | (required) | Hostname or IP the data member's channel listens on. |
| `channel_port` | u16 | (required) | Data member channel port. |
| `client_port` | u16 | (required) | Data member client port. |
| `replication_port` | u16 | (required) | Data member replication port. |

`[[data_plane.placements]]` fields:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `rule` | string | (required) | Partition rule name. |
| `partition` | string | (required) | Partition name. |
| `group` | string | (required) | The referenced data-group name. |
| `worker_index` | u32 | (required) | Worker slot index within the group. |

### `[backup]`

Backup and point-in-time recovery. When the section is absent the server runs backup-free: WAL prefix GC reclaims chunks without archiving them. When present, sealed WAL chunks are continuously archived to `archive_dir` before GC may reclaim them.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `archive_dir` | string | (absent) | Directory WAL chunks are archived to (continuous archiving). |
| `backup_dir` | string | (absent) | Directory base backups are written to (`mcli backup`). |
| `archive_mode` | string | `"on"` | Archiving mode: `"off"`, `"on"`, or `"always"`. |

## Compatibility notes

- The configuration carries an explicit `version = 1`; unknown versions are an error, not a silent downgrade.
- Nodes, data groups and the cluster are identified by names; internal ids are derived as u64 via the FNV-1a 64 hash. The hash is part of the contract; changing it is a breaking change.
- `serde(default)` ensures missing fields use their defaults.

## Upgrade and rollback rules

- **Upgrade:** The format version is currently 1. A future breaking change bumps `version` and must provide an offline config migration tool.
- **Rollback:** A `version` mismatch fails parsing and returns a decode error; it does not modify the file.
- **Migration:** No migration tool is required for additive changes. For breaking changes, an offline config migration tool must be provided.

## Deprecation policy

- A breaking change bumps the `version` field and adds a migration tool.

## References

- Parser: [`mudu_runtime/src/backend/mudud_cfg/mod.rs`](../../../crates/db-kernel/mudu_runtime/src/backend/mudud_cfg/mod.rs)
- Example config: [`mudud init-cfg`](../running_mudud.md)
