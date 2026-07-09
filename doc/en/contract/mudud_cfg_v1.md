# MuduDB Server Configuration Contract v1

## Scope

This document specifies the `mudud.cfg` server configuration file format. The file controls server listen ports, execution mode, runtime paths, and io_uring behavior.

## Version history

| Version | Date | Summary |
|---------|------|---------|
| 1 | 2026-6-25 | Initial TOML configuration. No explicit `version` field; compatibility is implicit through serde defaults and aliases. |

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
| `server_mode` | string | `"IOUring"` | `"Legacy"`, `"IOUring"`, or `"Tokio"`. |
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
| `page_size` | usize | `4096` | Database page size in bytes. This is a persistent setting; changing it for an existing database requires migration or re-initialization. |

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

- Parser: [`mudu_runtime/src/backend/mudud_cfg.rs`](../../../mudu_runtime/src/backend/mudud_cfg.rs)
- Example config: [`doc/cfg/mudud.cfg`](../../cfg/mudud.cfg)
