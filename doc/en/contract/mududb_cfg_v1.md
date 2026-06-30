# MuduDB Server Configuration Contract v1

## Scope

This document specifies the `mududb_cfg.toml` server configuration file format. The file controls server listen ports, execution mode, runtime paths, and io_uring behavior.

## Version history

| Version | Date | Summary |
|---------|------|---------|
| 1 | 2026-6-25 | Initial TOML configuration. No explicit `version` field; compatibility is implicit through serde defaults and aliases. |

## File location

The server loads the configuration from:

1. The path provided by `--cfg /path/to/mududb_cfg.toml`, or
2. `${HOME}/.mududb/mududb_cfg.toml` by default.

If the file does not exist, the server creates it with default values on first start.

## Configuration fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `mpk_path` | string | temp dir | Path to the application package directory. |
| `db_path` | string | temp dir | Path to the database directory. Alias: `data_path`. |
| `listen_ip` | string | `127.0.0.1` | IP address to listen on. |
| `http_listen_port` | u16 | `8300` | HTTP management API port. |
| `http_worker_threads` | usize | `1` | HTTP worker thread count. |
| `pg_listen_port` | u16 | `5432` | PostgreSQL wire protocol port. |
| `component_target` | string | `p2` | Wasm component ABI target. Allowed: `p2`, `p3`. |
| `enable_async` | boolean | `true` | Enable the WASI component runtime. |
| `server_mode` | integer | `0` | `0` = Legacy, `1` = IOUring, `2` = Tokio. |
| `tcp_listen_port` | u16 | `9527` | TCP framed protocol port. |
| `tcp_multi_port` | boolean | `false` | One TCP listener per worker. |
| `worker_threads` | usize | `0` | Worker thread count. `0` means use available parallelism. Alias: `io_uring_worker_threads`. |
| `io_uring_ring_entries` | u32 | `1024` | io_uring completion queue depth. |
| `io_uring_accept_multishot` | boolean | `true` | Enable io_uring accept multishot. |
| `io_uring_recv_multishot` | boolean | `true` | Enable io_uring recv multishot. |
| `io_uring_enable_fixed_buffers` | boolean | `false` | Enable io_uring fixed buffers. |
| `io_uring_enable_fixed_files` | boolean | `false` | Enable io_uring fixed files. |
| `routing_mode` | integer | `0` | `0` = ConnectionId, `1` = PlayerId, `2` = RemoteHash. |
| `io_uring_log_chunk_size` | u64 | `64 * 1024 * 1024` | io_uring log chunk size in bytes. |
| `page_size` | usize | `4096` | Database page size in bytes. This is a persistent setting; changing it for an existing database requires migration or re-initialization. |

## Compatibility notes

- There is currently no explicit `version` field in the file. The format version is implicit and derived from the set of fields recognized by the parser.
- `serde(default)` ensures that missing fields use the default values, allowing older config files to load on newer binaries.
- Field aliases (`data_path` → `db_path`, `io_uring_worker_threads` → `worker_threads`) preserve backward compatibility with older config files.

## Upgrade and rollback rules

- **Upgrade:** When a v2 config format is introduced, an explicit `version = 2` field will be required. New optional fields may be added to v1 without bumping the version if they use `serde(default)`.
- **Rollback:** A newer binary can read older config files because of defaults and aliases. A v1-only binary encountering a v2 config will fail to parse unknown fields and return a decode error; it will not modify the file.
- **Migration:** No migration tool is required for additive changes. For breaking changes, an offline config migration tool must be provided.

## Deprecation policy

- Aliases may be removed only after two full release cycles and after updating all documentation and example configs.
- A new explicit `version` field will be added when a breaking change is introduced.

## References

- Parser: [`mudu_runtime/src/backend/mududb_cfg.rs`](../../../mudu_runtime/src/backend/mududb_cfg.rs)
- Example config: [`doc/cfg/mududb_cfg.toml`](../../cfg/mududb_cfg.toml)
