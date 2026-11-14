# MuduDB Server Configuration Contract v1

## Scope

This document specifies the `mudud.cfg` server configuration file format. The file controls server listen ports, execution mode, runtime paths, and io_uring behavior.

## Version history

| Version | Date | Summary |
|---------|------|---------|
| 1 | 2026-6-25 | Initial TOML configuration. No explicit `version` field; compatibility is implicit through serde defaults and aliases. |
| 1 | 2026-10-12 | Documented existing backward-compatible fields (`wal_flush_max_wait_us`, `wal_sync_mode`, `wal_sync_interval_ms`, `[invocation_limits]`, `[replication]`, `[backup]`); corrected the documented `server_mode` default to `"Tokio"`. No format change. |

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

### `[replication]`

Primary-replica streaming replication and Raft fault tolerance. When the section is absent the server runs as a single node with no replication components started. Exactly one of the `[replication.stream]` / `[replication.raft]` subsections must be present; the present subsection selects the mode.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `node_id` | u64 | (required) | Identifier of this node. Persisted to `<db_path>/node_id` on first boot; a mismatch with the persisted value is a startup error. |
| `listen_port` | u16 | (required) | Port the replication and Raft channel listens on. |
| `repl_flow_window_segments` | u64 | `64` | Flow-control window of the replication stream, in segments. |
| `snapshot_wal_threshold_bytes` | u64 | `1073741824` | Committed WAL bytes beyond the last snapshot that trigger the next per-worker snapshot. |
| `gc_check_interval_secs` | u64 | `60` | Interval between WAL/staging GC watermark evaluations, in seconds. |
| `stream` | table | (absent) | `[replication.stream]` subsection; selects streaming replication mode. |
| `raft` | table | (absent) | `[replication.raft]` subsection; selects Raft consensus mode. |
| `peers` | array of tables | `[]` | `[[replication.peers]]` static cluster members; must contain this node's `node_id`. |

`[replication.stream]` fields:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `primary` | boolean | (required) | Whether this node is the configured primary; exactly one cluster member sets `true`. |
| `sync` | boolean | (required) | Whether the primary waits for a replica's durable ack before answering a commit. |
| `sync_timeout_ms` | u64 | `10000` | Synchronous replication wait timeout in milliseconds. |

`[replication.raft]` fields:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `raft_tick_ms` | u64 | `50` | Raft state machine tick interval in milliseconds. |
| `election_timeout_ticks` | u64 | `500` | Raft election timeout in ticks. |

`[[replication.peers]]` fields:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `node_id` | u64 | (required) | Node identifier of the peer. |
| `host` | string | (required) | Hostname or IP address the peer's replication channel listens on. |
| `port` | u16 | (required) | Port the peer's replication channel listens on. |

### `[backup]`

Backup and point-in-time recovery. When the section is absent the server runs backup-free: WAL prefix GC reclaims chunks without archiving them. When present, sealed WAL chunks are continuously archived to `archive_dir` before GC may reclaim them.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `archive_dir` | string | (absent) | Directory WAL chunks are archived to (continuous archiving). |
| `backup_dir` | string | (absent) | Directory base backups are written to (`mcli backup`). |
| `archive_mode` | string | `"on"` | Archiving mode: `"off"`, `"on"`, or `"always"`. |

## Compatibility notes

- There is currently no explicit `version` field in the file. The format version is implicit and derived from the set of fields recognized by the parser.
- `serde(default)` ensures that missing fields use the default values, allowing older config files to load on newer binaries as long as they use the current field names.

## Upgrade and rollback rules

- **Upgrade:** When a v2 config format is introduced, an explicit `version = 2` field will be required. New optional fields may be added to v1 without bumping the version if they use `serde(default)`.
- **Rollback:** A newer binary can read older config files because of defaults, provided they use the current field names. A v1-only binary encountering a v2 config will fail to parse unknown fields and return a decode error; it will not modify the file.
- **Migration:** No migration tool is required for additive changes. For breaking changes, an offline config migration tool must be provided.

## Deprecation policy

- A new explicit `version` field will be added when a breaking change is introduced.

## References

- Parser: [`mudu_runtime/src/backend/mudud_cfg/mod.rs`](../../../crates/db-kernel/mudu_runtime/src/backend/mudud_cfg/mod.rs)
- Example config: [`mudud init-cfg`](../running_mudud.md)
