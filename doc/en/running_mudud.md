# Running `mudud`

This guide explains how to configure and run the `mudud` MuduDB server.

## What is `mudud`?

`mudud` is the MuduDB server process. It hosts the database kernel, runs installed MPK packages, exposes a TCP protocol endpoint for clients, an HTTP management endpoint, and a PostgreSQL-compatible wire protocol port.

## Prerequisites

- `mudud` and `mcli` are installed and in `PATH`.
- You have chosen a working directory where `mudud` will read `mudud.cfg` and store data.
- On Linux, `io_uring` mode requires `liburing-dev` at runtime.

## Configuration file

`mudud` searches for the configuration file in this order:

1. The path given by `--cfg` / `-c` (if supplied). This path is used exactly as given.
2. `./mudud.cfg` in the current working directory.
3. `~/.mududb/mudud.cfg` in the user's home directory.

The first file that exists is loaded. If none of them exist, `mudud` returns a `NotFound` error and does not start.

### Generate a default config

To write a default `./mudud.cfg` without starting the server:

```bash
mudud init-cfg
```

### Use a custom config path

```bash
mudud serve --cfg /path/to/mudud.cfg
```

### Example `mudud.cfg`

```toml
# Directory containing .mpk application packages.
mpk_path = "./mpk"

# Directory for database storage files.
db_path = "./data"

# IP address to listen on.
listen_ip = "127.0.0.1"

# HTTP management API port.
http_listen_port = 8300

# Number of HTTP worker threads.
http_worker_threads = 1

# PostgreSQL wire protocol port.
pg_listen_port = 5432

# Internal TCP port used by the MuduDB protocol client.
tcp_listen_port = 9527

# Server execution mode: "Legacy", "IOUring" (recommended on Linux), or "Tokio".
server_mode = "Tokio"

# Number of worker threads. 0 means auto-detect CPU cores.
worker_threads = 0

# io_uring completion queue ring entries.
io_uring_ring_entries = 1024

# Enable io_uring accept/receive multishot optimizations.
io_uring_accept_multishot = true
io_uring_recv_multishot = true

# Enable fixed buffers/files for io_uring (experimental).
io_uring_enable_fixed_buffers = false
io_uring_enable_fixed_files = false

# TCP routing mode: "ConnectionId", "PlayerId", or "RemoteHash".
routing_mode = "ConnectionId"

# Async runtime support.
enable_async = true

# Use multiple consecutive TCP ports for workers.
tcp_multi_port = false

# log chunk size in bytes.
log_chunk_size = 67108864

# Database page size in bytes. Persistent: changing it requires re-initialization.
page_size = 4096
```

### Configuration reference

| Field | Default | Description |
|-------|---------|-------------|
| `mpk_path` | temp dir | Directory where `.mpk` application packages are stored. |
| `db_path` | temp dir | Directory for database storage files. |
| `listen_ip` | `127.0.0.1` | IP address the server listens on. |
| `http_listen_port` | `8300` | HTTP management API port. |
| `http_worker_threads` | `1` | Number of HTTP worker threads. |
| `pg_listen_port` | `5432` | PostgreSQL wire protocol port. |
| `tcp_listen_port` | `9527` | MuduDB TCP protocol port used by `mcli`. |
| `server_mode` | `Tokio` | Backend mode: `Legacy`, `IOUring`, or `Tokio`. Linux users should prefer `IOUring`. |
| `worker_threads` | `0` | Number of worker threads. `0` auto-detects CPU cores. |
| `io_uring_ring_entries` | `1024` | io_uring completion queue ring entries. |
| `io_uring_accept_multishot` | `true` | Enable io_uring accept multishot. |
| `io_uring_recv_multishot` | `true` | Enable io_uring receive multishot. |
| `io_uring_enable_fixed_buffers` | `false` | Enable io_uring fixed buffers (experimental). |
| `io_uring_enable_fixed_files` | `false` | Enable io_uring fixed files (experimental). |
| `routing_mode` | `ConnectionId` | TCP connection routing strategy: `ConnectionId`, `PlayerId`, or `RemoteHash`. |
| `enable_async` | `true` | Enable async runtime support for WASM procedures. |
| `tcp_multi_port` | `false` | Use multiple consecutive TCP ports for workers. |
| `log_chunk_size` | `67108864` | io_uring log chunk size in bytes. |
| `page_size` | `4096` | Database page size. Persistent: changing it for an existing database requires re-initialization. |

## Starting the server

### Default start

```bash
ulimit -n 65535
mudud
```

If `./mudud.cfg` does not exist in the current directory, `mudud` then checks `~/.mududb/mudud.cfg`. If neither file exists, create one with `mudud init-cfg` before starting the server.

### Start with a custom config

```bash
ulimit -n 65535
mudud serve --cfg ./config/mudud.cfg
```

### What to expect

After starting, `mudud` logs the effective configuration and opens three listeners:

- TCP protocol: `listen_ip:tcp_listen_port` (default `127.0.0.1:9527`)
- HTTP management: `listen_ip:http_listen_port` (default `127.0.0.1:8300`)
- PostgreSQL wire protocol: `listen_ip:pg_listen_port` (default `127.0.0.1:5432`)

## Stopping the server

Send `SIGINT` (`Ctrl+C`) or `SIGTERM` to the process. `mudud` performs a graceful shutdown, waiting for in-flight work to finish before exiting.

## Verifying the server

Use `mcli` to check the HTTP management endpoint:

```bash
mcli --http-addr 127.0.0.1:8300 app-list
mcli --http-addr 127.0.0.1:8300 server-topology
```

If the commands return JSON output, the server is running and reachable.

## Common issues

- **Address already in use**: Another process is using one of the configured ports. Change the conflicting port in `mudud.cfg`.
- **Permission denied**: You may not have permission to bind to the configured `listen_ip` or port. Use `127.0.0.1` and ports above `1024` for local development.
- **Too many open files**: Raise the file descriptor limit with `ulimit -n 65535` before starting.
- **io_uring not available**: If you are not on Linux or `liburing-dev` is missing, switch `server_mode` to `Tokio`.

## Next steps

- [mcli management interface](mcli_admin.md) — install, list, and invoke application packages.
- [Your first MPK package](your_first_mpk.md) — build and install a complete example.
- [Core concepts](concepts.md) — learn about Mudu Procedures, MPK packages, and the runtime model.
