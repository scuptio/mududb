# YCSB Example

This example provides a YCSB-style key/value benchmark on top of the Mudu key/value syscall API.

It contains:

- synchronous procedure implementations for YCSB operations
- async generated procedure implementations for wasm packaging
- a native benchmark runner for local execution through `sys_interface` standalone adapter

## Target Docs

- [SQLite deployment and test guide](./doc/sqlite.md)
- [PostgreSQL deployment and test guide](./doc/postgresql.md)
- [MySQL deployment and test guide](./doc/mysql.md)
- [MuduDB TCP connection deployment and test guide](doc/mududb.md)
- [Mudu Package build guide](./doc/wasm_package.md)

Procedures:

- `ycsb_insert`
- `ycsb_read`
- `ycsb_update`
- `ycsb_scan`
- `ycsb_read_modify_write`

## Native Benchmark Runner

The native runner uses the standalone adapter path.
For SQLite / PostgreSQL / MySQL connections it does not require `mudud` or `mudu_runtime`.
For `mudud://...` connections it still requires a running `mudud` instance.
It also supports range partitioning with `--partition-count`, and fixes each worker/session to one topology partition slot resolved from the backend.
With `--enable-async`, the benchmark runs workers on a multithread Tokio runtime instead of using one OS thread per worker.
With `--enable-transaction`, the benchmark wraps each benchmarked operation with `begin transaction`, `commit transaction`, and `rollback transaction` on failure, using the same session as the key/value request path.
By default, transaction wrapping applies only to the run phase.
Use `--transaction-load` if you also want load-phase inserts wrapped in explicit transactions.

Example with SQLite:

```bash
export MUDU_CONNECTION="sqlite://./ycsb.db"
cargo run -p ycsb --features benchmark-runner --bin ycsb-benchmark -- \
  --workload a \
  --connection-count 4 \
  --partition-count 4 \
  --record-count 10000 \
  --operation-count 10000 \
  --field-length 256
```

Example with PostgreSQL:

```bash
export MUDU_CONNECTION="postgres://postgres:postgres@127.0.0.1:5432/ycsb"
cargo run -p ycsb --features benchmark-runner --bin ycsb-benchmark -- \
  --workload b
```

Example with MySQL:

```bash
export MUDU_CONNECTION="mysql://root:root@127.0.0.1:3306/ycsb"
cargo run -p ycsb --features benchmark-runner --bin ycsb-benchmark -- \
  --workload f
```

Example with mudud TCP:

```bash
export MUDU_CONNECTION="mudud://127.0.0.1:9000/ycsb?http_addr=127.0.0.1:8300"
cargo run -p ycsb --features benchmark-runner --bin ycsb-benchmark -- \
  --workload f \
  --enable-async \
  --connection-count 8 \
  --partition-count 8
```

Example with explicit per-operation transaction wrapping:

```bash
export MUDU_CONNECTION="mudud://127.0.0.1:9527/ycsb?http_addr=127.0.0.1:8300"
cargo run -p ycsb --features benchmark-runner --bin ycsb-benchmark -- \
  --workload a \
  --connection-count 8 \
  --partition-count 8 \
  --enable-transaction
```

Example with transaction wrapping enabled for both load and run:

```bash
export MUDU_CONNECTION="mudud://127.0.0.1:9527/ycsb?http_addr=127.0.0.1:8300"
cargo run -p ycsb --features benchmark-runner --bin ycsb-benchmark -- \
  --workload a \
  --record-count 10000 \
  --operation-count 10000 \
  --enable-transaction \
  --transaction-load
```

Supported workloads:

- `a`: 50% read, 50% update
- `b`: 95% read, 5% update
- `c`: 100% read
- `e`: 95% scan, 5% insert
- `f`: 50% read, 50% read-modify-write

Important runner flags:

- `--enable-async`: run the benchmark worker path on Tokio async tasks
- `--enable-transaction`: wrap each benchmarked operation in an explicit transaction on the same session
- `--transaction-load`: extend transaction wrapping to the load phase

## Build `.mpk`

```bash
cd example/ycsb
cargo make
```
