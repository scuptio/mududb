# TPC-C Example

> This is a **benchmark example**, not a beginner tutorial. If you are new to MuduDB, start with [`example/wallet/readme.md`](../wallet/readme.md) or the [Your First MPK tutorial](../../doc/en/your_first_mpk.md).

This example provides a portable TPC-C style benchmark package for the `mudu`
toolchain. It stays intentionally smaller than a full audited specification
implementation, but the SQL schema and `new_order` flow now track the canonical
TPC-C model more closely:

- shared `item` catalog plus per-warehouse `stock`
- district-scoped order id allocation
- multi-line `new_order`
- supplier warehouse per order line
- `o_all_local`, `s_remote_cnt`, `o_entry_d`, `ol_delivery_d`

## Dependency checklist

Before building, make sure you have:

- [ ] `rustup` + `cargo` + `wasm32-wasip2` target
- [ ] `cargo-make`
- [ ] `python3` with `tomli-w` and `toml`
- [ ] `mgen`, `mtp`, `mpm-build`, `mudud`, `mcli` in `PATH` (install via `python script/build/install_binaries.py` from the repo root)

The SQL avoids SQLite-only constructs such as `INSERT OR REPLACE` / `INSERT OR IGNORE`
so the example remains compatible with MuduDB, SQLite, and PostgreSQL. Where a
vendor-specific shortcut would normally be used, the Rust procedure code performs
the logic explicitly.

For the same reason, the benchmark runner no longer depends on the `batch`
syscall for schema initialization. It splits `ddl.sql` / `init.sql` into
individual statements and executes them one by one through `mudu_command`,
so the interactive Rust benchmark path can run unchanged across standalone SQLite,
PostgreSQL, MySQL, and `mudud` adapters.

Supported transaction families:

- `tpcc_seed`
- `tpcc_new_order`
- `tpcc_payment`
- `tpcc_order_status`
- `tpcc_delivery`
- `tpcc_stock_level`

Benchmark runner modes:

- `--mode interactive`
  Directly calls `tpcc::rust::procedure::*` through `sys_interface` and `mudu_adapter`
- `--mode stored-procedure`
  Connects to a running `mudud` TCP server through the `mcli` client library and invokes the installed `.mpk` procedures

```bash
cargo run -p tpcc --features benchmark-runner --bin tpcc-benchmark -- --mode interactive --operation-count 1000
```

Interactive mode examples:

```bash
export MUDU_CONNECTION="sqlite://./tpcc.db"
cargo run -p tpcc --features benchmark-runner --bin tpcc-benchmark -- --mode interactive --operation-count 1000
```

```bash
export MUDU_CONNECTION="postgres://postgres:postgres@127.0.0.1:5432/tpcc"
cargo run -p tpcc --features benchmark-runner --bin tpcc-benchmark -- --mode interactive --operation-count 1000
```

```bash
export MUDU_CONNECTION="mysql://root:root@127.0.0.1:3306/tpcc"
cargo run -p tpcc --features benchmark-runner --bin tpcc-benchmark -- --mode interactive --operation-count 1000
```

```bash
export MUDU_CONNECTION="mudud://127.0.0.1:9527/tpcc?http_addr=127.0.0.1:8300"
cargo run -p tpcc --features benchmark-runner --bin tpcc-benchmark -- --mode interactive --operation-count 1000
```

Stored procedure mode prerequisites:

1. Build the package:

```bash
cd example/tpcc
cargo make package
```

This produces `target/wasm32-wasip2/release/tpcc.mpk`.

2. Start a `mudud` server with TCP and HTTP management ports enabled.

3. Run the benchmark in stored procedure mode:

```bash
cargo run -p tpcc --features benchmark-runner --bin tpcc-benchmark -- \
  --mode stored-procedure \
  --app-name tpcc \
  --tcp-addr 127.0.0.1:9527 \
  --http-addr 127.0.0.1:8300 \
  --mpk target/wasm32-wasip2/release/tpcc.mpk \
  --operation-count 1000
```

If the package has already been installed into the target `mudud`, omit `--mpk` and reuse the installed app:

```bash
cargo run -p tpcc --features benchmark-runner --bin tpcc-benchmark -- \
  --mode stored-procedure \
  --app-name tpcc \
  --tcp-addr 127.0.0.1:9527 \
  --http-addr 127.0.0.1:8300 \
  --operation-count 1000
```

Notes:

- `src/rust/` only contains synchronous source procedures.
- async procedure code is expected to be produced by the `mpm-build` transpile stage into `src/generated/`.
- in `--mode interactive`, the host benchmark binary exercises the synchronous Rust implementation.
- in `--mode stored-procedure`, the benchmark invokes the transpiled `.mpk` procedures over mudud TCP.

Package build:

```bash
cargo make package
```
