# Your First MPK

This tutorial walks you through building and installing your first MuduDB package (`.mpk`) using the existing `example/wallet` project. By the end you will know how to turn a compiled WebAssembly procedure into an installed, invocable package inside `mudud`.

## What you will learn

- What an MPK package contains (DDL, descriptors, and a WASM module).
- How to build an `.mpk` with `mpm-build`.
- How to install an `.mpk` into a running MuduDB server with `mpm-install`.
- How to verify and invoke the installed procedures with `mcli`.

## Prerequisites

Complete the environment setup from [`how_to_start.md`](how_to_start.md) and make sure `mpm-build`, `mpm-install`, `mudud`, and `mcli` are in `PATH`.

## 1. The wallet example

`example/wallet` is a complete Rust project that defines several Mudu Procedures such as `create_user`, `deposit`, and `transfer_funds`. The source lives in `example/wallet/src/rust/procedures.rs` and the schema lives in `example/wallet/sql/ddl.sql`.

If you want to learn how procedures are authored, see [`procedure.md`](procedure.md). This tutorial focuses on packaging and installing the result.

## 2. Build the package

From `example/wallet`:

```bash
cargo make package
```

This single command runs the full pipeline:

1. Reinstalls the workspace CLI tools (`mgen`, `mtp`, `mpm-build`, etc.) from source so the toolchain stays in sync with the current commit.
2. `mgen entity ...` — generates typed Rust entity bindings from `sql/ddl.sql`.
3. `mtp` transpiles synchronous procedure code into async WebAssembly wrappers.
4. Formats the generated sources with `cargo fmt`.
5. `cargo build --target wasm32-wasip2 --release` — compiles the generated code to a WebAssembly component.
6. `mpm-build create ...` — packages the DDL, descriptors, and WASM file into `target/wasm32-wasip2/release/wallet.mpk`.

You can also invoke `mpm-build create` directly if you already have the WASM file and descriptors; `cargo make package` is just a convenient wrapper used by the example.

## 3. Start the server

Start `mudud` in one terminal:

```bash
ulimit -n 65535
mudud
```

## 4. Install the package

In another terminal, install the `.mpk` with `mpm-install`:

```bash
mpm-install target/wasm32-wasip2/release/wallet.mpk
```

By default `mpm-install` sends the package to the MuduDB HTTP management endpoint at `127.0.0.1:8300`. You can specify a different server with `--server`:

```bash
mpm-install --server 192.168.1.100:8300 target/wasm32-wasip2/release/wallet.mpk
```

## 5. Verify the installation

```bash
mcli --http-addr 127.0.0.1:8300 app-list
mcli --http-addr 127.0.0.1:8300 app-detail --app wallet
```

## 6. Invoke the procedures

Create two users:

```bash
mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke \
  --app wallet --module wallet --proc create_user \
  --json '{"user_id": 1001, "name": "Alice", "email": "alice@example.com"}'

mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke \
  --app wallet --module wallet --proc create_user \
  --json '{"user_id": 1002, "name": "Bob", "email": "bob@example.com"}'
```

Deposit and transfer:

```bash
mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke \
  --app wallet --module wallet --proc deposit \
  --json '{"user_id": 1001, "amount": 5000}'

mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke \
  --app wallet --module wallet --proc transfer_funds \
  --json '{"from_user_id": 1001, "to_user_id": 1002, "amount": 1200}'
```

Check balances:

```bash
mcli --addr 127.0.0.1:9527 shell --app wallet
```

```sql
SELECT user_id, balance FROM wallets WHERE user_id = 1001;
SELECT user_id, balance FROM wallets WHERE user_id = 1002;
\q
```

## Next steps

- Read [`concepts.md`](concepts.md) for the terminology used above.
- Read [`procedure.md`](procedure.md) to learn how to write your own procedures.
- Try the [`example/key-value`](../example/key-value/README.md) example for a smaller project that uses the key/value API.
