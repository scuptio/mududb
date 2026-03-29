# Standalone Adapter Usage

`sys_interface` provides a standalone debug path behind the `standalone-adapter` feature.
When this feature is enabled on non-`wasm32` targets, syscall implementations are routed to `mudu_adapter` instead of returning `NotImplemented`.

This path is intended for local user-code debugging.
For SQLite / PostgreSQL / MySQL standalone connections, it does not require `mudu_runtime`, `mudud`, or the kernel/backend stack to be running.
For `mudud://...` connections, a running `mudud` instance is still required.

## Enable It

Enable the feature on `sys_interface` in the crate that uses the syscall APIs.
Use `sync_api` for synchronous native code, or add `async` and use `async_api` for asynchronous native code.

```toml
[dependencies]
sys_interface = { path = "../sys_interface", features = ["standalone-adapter"] }
```

For async native code:

```toml
[dependencies]
sys_interface = { path = "../sys_interface", features = ["standalone-adapter", "async"] }
```

After that, native builds can directly call the standalone adapter through:

- `sys_interface::sync_api::*` for synchronous code
- `sys_interface::async_api::*` for asynchronous code

## Connection Configuration

The standalone adapter is configured with a single environment variable:

```bash
MUDU_CONNECTION=...
```

The adapter parses this value, selects the driver, and forwards the connection parameters to the corresponding database client.

Supported forms:

- `sqlite://./mudu_debug.db`
- `sqlite:/tmp/mudu_debug.db`
- `/tmp/mudu_debug.db`
- `postgres://user:pass@127.0.0.1:5432/app_db`
- `postgresql://user:pass@127.0.0.1:5432/app_db`
- `mysql://user:pass@127.0.0.1:3306/app_db`
- `mudud://127.0.0.1:9000/app_name`
- `mudud://127.0.0.1:9000/app_name?http_addr=127.0.0.1:8300`
- `mudud://127.0.0.1:9000/app_name?http_addr=127.0.0.1:8300&async_session_loop=true`

If `MUDU_CONNECTION` is not set, the default is:

```bash
sqlite://./mudu_debug.db
```

## SQLite Adapter

Example:

```bash
export MUDU_CONNECTION="sqlite://./mudu_debug.db"
cargo run
```

Behavior:

- creates the SQLite database file if it does not exist
- creates internal tables used by the adapter
- stores session metadata in `mudu_session`
- stores KV syscall data in `mudu_kv`
- executes `query` and `command` directly against the same SQLite database

Use this mode when you want the lightest local debug setup.

## PostgreSQL Adapter

Example:

```bash
export MUDU_CONNECTION="postgres://postgres:postgres@127.0.0.1:5432/app_db"
cargo run
```

Behavior:

- connects with the PostgreSQL client driver
- creates `mudu_kv` if needed
- keeps session state in the local adapter process
- stores KV syscall data in PostgreSQL
- executes `query` and `command` directly against PostgreSQL

Notes:

- this is intended for local debugging, not production deployment
- parameters are currently expanded into SQL text for debug-oriented execution

## MySQL Adapter

Example:

```bash
export MUDU_CONNECTION="mysql://root:root@127.0.0.1:3306/app_db"
cargo run
```

Behavior:

- connects with the MySQL client driver
- creates `mudu_kv` if needed
- keeps session state in the local adapter process
- stores KV syscall data in MySQL
- executes `query` and `command` directly against MySQL

Notes:

- this is intended for local debugging, not production deployment
- parameters are currently expanded into SQL text for debug-oriented execution
- the adapter currently uses a bounded binary key column for `mudu_kv`

## MuduDB TCP Adapter

Example:

```bash
export MUDU_CONNECTION="mudud://127.0.0.1:9000/app1?http_addr=127.0.0.1:8300"
cargo run
```

Behavior:

- connects to `mudud` through the TCP protocol client in `mudu_cli`
- creates a remote session on `open`
- closes the remote session on `close`
- forwards `get` / `put` / `range` to the remote backend session
- forwards `query` / `command` to the configured `app_name`

Notes:

- the `app_name` is part of `MUDU_CONNECTION`
- `http_addr` points to the management HTTP API and is used by clients that need server topology
- for the `mudud` adapter, `mudu_open_argv` routes by persistent worker OID instead of worker index
- `async_session_loop=true` enables a single-thread async manager that owns multiple remote sessions
- query result values are currently mapped back as strings
- this mode is useful when you want syscall compatibility against a running `mudud` backend

## Session and KV Semantics

All adapters currently implement a simple debug-oriented model:

- `open` creates a logical session in the adapter
- `close` removes the logical session from the adapter
- `get` / `put` / `range` operate on a shared `mudu_kv` table
- no MVCC is implemented
- no kernel transaction semantics are emulated

This is sufficient for local functional debugging of user code, but should not be treated as a faithful backend replacement.

## Limitations

Current limitations of the standalone adapter path:

- no MVCC
- no kernel worker model
- no backend scheduling/runtime behavior
- `NULL` result values are not supported
- SQL parameter handling is debug-oriented and not yet equivalent to production syscall execution

## Quick Check

You can validate the adapter integration with:

```bash
cargo check -p mudu_adapter -p sys_interface --features standalone-adapter
cargo check -p mudu_adapter -p sys_interface --features 'standalone-adapter async'
```
