---
name: run-mududb
description: >
  Build, configure, start, and test MuduDB (the io_uring database server).
  Use this skill whenever the user wants to run mudud, start the database server,
  test the wallet example, run CRUD operations, debug server crashes, or verify
  that MuduDB is working. Also use for questions about mududb_cfg.toml configuration,
  mcli commands, or HTTP API invocation of procedures.
---

# Run MuduDB

MuduDB is an io_uring-based database server with WASM procedure execution. This skill covers the full lifecycle: build, configure, start, test, and debug.

## Quick Start

For a one-command build + install + start + test cycle, run:

```bash
./script/shell/run_test.sh
```

This script handles everything: config, server startup, wallet install, CRUD tests, and cleanup.

## Step-by-Step Guide

### 1. Build

```bash
cargo build --release
```

Binaries land in `target/release/`: `mudud` (server), `mcli` (client), `mpk`, `mgen`, `mtp`.

### 2. Install Binaries

Copy to `~/.cargo/bin/` so they're on PATH. **Kill any running mudud first** (the binary will be busy otherwise):

```bash
pkill -9 mudud 2>/dev/null || true
sleep 1
cp target/release/mudud ~/.cargo/bin/
cp target/release/mcli ~/.cargo/bin/
```

Or use the project script: `python script/build/install_binaries.py`

### 3. Configure

MuduDB reads `~/.mududb/mududb_cfg.toml`. Key settings for io_uring mode:

```toml
mpk_path = "/tmp/mudu_test/mpk"       # directory with .mpk app packages
db_path = "/tmp/mudu_test/data"        # database storage directory
listen_ip = "127.0.0.1"
http_listen_port = 8300                # management API (REST)
tcp_listen_port = 9527                 # internal protocol (io_uring workers)
pg_listen_port = 5432                  # PostgreSQL wire protocol
server_mode = 1                        # 0=Legacy, 1=IOUring
worker_threads = 2                     # 0=auto-detect CPU cores
routing_mode = 2                       # 0=ConnectionId, 1=PlayerId, 2=RemoteHash
enable_async = true
http_worker_threads = 1
```

Always create the data and mpk directories before starting:

```bash
mkdir -p /tmp/mudu_test/data /tmp/mudu_test/mpk
```

### 4. Start the Server

```bash
RUST_LOG=mudu_runtime=info mudud > /tmp/mudu_test/server.log 2>&1 &
```

Wait for both HTTP (8300) and TCP (9527) ports to be ready:

```bash
for i in $(seq 1 30); do
    curl -s http://127.0.0.1:8300/ > /dev/null 2>&1 && echo "Ready (${i}s)" && break
    sleep 1
done
```

The server log is at the path you redirected to. Check it if the server fails to start.

### 5. Install an App

```bash
mcli --http-addr 127.0.0.1:8300 app-install --mpk testing/mpk/wallet.mpk
```

### 6. Query and Invoke

**SQL queries** go through `mcli command` over TCP:

```bash
mcli command --json '{"app_name":"wallet","sql":"SELECT user_id, name FROM users"}' --compact --no-table
```

**Procedure invocations** go through the HTTP API:

```bash
curl -s -X POST "http://127.0.0.1:8300/mudu/app/invoke/wallet/wallet/create_user" \
  -H "Content-Type: application/json" \
  -d '{"user_id":3,"name":"Charlie","email":"charlie@test.com"}'
```

The URL pattern is: `/mudu/app/invoke/{app_name}/{module_name}/{procedure_name}`

### 7. Stop the Server

```bash
pkill -9 mudud
```

## Helper Scripts

The repo includes automation scripts:

| Script | Purpose |
|--------|---------|
| `script/shell/run_test.sh` | Full integration test: build, start, install wallet, CRUD, cleanup |
| `script/shell/t.sh` | Quick restart: kill, install binary, start with clean data |
| `script/shell/debug_test.sh` | Detailed test with step-by-step checks and preserved temp dir |

## Common Issues

### "Text file busy" when copying mudud binary
A mudud process is still running. Kill it first: `pkill -9 mudud`

### "Connection refused" during startup
The io_uring TCP server takes a moment to start. The DDL init thread retries automatically (up to 15s). If it persists, check the server log.

### "no such table" on queries
The DDL initialization may have failed. Check the server log for "DDL execution failed" warnings. Restart with a clean data directory.

### Server crashes (segfault) on procedure invoke
Check the server log. Common cause: database not initialized. Ensure the DDL init thread completed successfully (look for "DDL executed successfully" in logs).

### SQL syntax errors
The built-in SQL parser (`sql_parser` based on `tree-sitter-sql`) has limited support. Known limitations:
- No `ORDER BY` support
- No `JOIN` support
- No `SELECT *` (enumerate columns explicitly)
- No subqueries

## Ports Reference

| Port | Protocol | Purpose |
|------|----------|---------|
| 8300 | HTTP/REST | Management API: app install, list, procedure invoke |
| 9527 | Custom TCP | Internal: io_uring worker communication, mcli queries |
| 5432 | PostgreSQL | Wire protocol (optional) |

## Architecture Notes

- **io_uring workers** each own their storage. DDL must execute through the TCP server (not direct file access).
- **DDL initialization** is deferred: the `ddl-init` thread waits for the TCP server, then executes CREATE TABLE via batch request.
- **HTTP management thread** runs on a standard tokio runtime (not io_uring). It proxies procedure invocations to the TCP server.
- Each procedure invocation: HTTP API -> TCP connection to io_uring worker -> WASM procedure execution -> SQL via worker-local storage.
