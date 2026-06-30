# Your First Mudu Procedure

This tutorial walks you through the lifecycle of a Mudu Procedure using the existing `example/wallet` project. By the end you will know how a handwritten Rust procedure becomes an installed, invocable package inside `mudud`.

## What you will learn

- How to mark a Rust function as a Mudu Procedure.
- How `mgen` generates typed entity bindings from SQL DDL.
- How `mtp` transpiles synchronous procedure code into async WebAssembly wrappers.
- How `mpk` packages the schema, descriptors, and WASM module.
- How to install and invoke the package with `mcli`.

## Prerequisites

Complete the environment setup from [`how_to_start.md`](how_to_start.md) and make sure `mgen`, `mtp`, `mpk`, `mudud`, and `mcli` are in `PATH`.

## 1. The handwritten procedure

Open `example/wallet/src/rust/procedures.rs`. The `transfer_funds` function is a typical Mudu Procedure:

```rust
/**mudu-proc**/
pub fn transfer_funds(xid: OID, from_user_id: i32, to_user_id: i32, amount: i32) -> RS<()> {
    // ... business logic using mudu_query / mudu_command ...
}
```

Key points:

- The `/**mudu-proc**/` marker tells the transpiler to export this function.
- The first parameter `xid: OID` is the session/transaction context provided by the runtime.
- The remaining parameters are ordinary Rust types (`i32`).
- The return type is `RS<()>` (alias for `Result<(), MuduError>`).

## 2. The schema

The wallet schema lives in `example/wallet/sql/ddl.sql`. For example:

```sql
CREATE TABLE wallets
(
    user_id    INT PRIMARY KEY,
    balance    INT,
    updated_at INT
);
```

`mgen` reads this file and generates typed Rust entities such as `Wallets` in `example/wallet/src/generated/`. These types are used by `mudu_query::<Wallets>(...)` to decode result rows.

## 3. Build the package

From `example/wallet`:

```bash
cargo make package
```

This single command runs the full pipeline:

1. `mgen entity ...` — generates `src/generated/` entity types and `package/type.desc.json` from `sql/ddl.sql`.
2. `python ../../script/build/transpiler.py ...` — runs `mtp` to transpile `src/rust/procedures.rs` into async wrappers in `src/generated/procedures.rs` and produces `package/package.desc.json`.
3. `cargo build --target wasm32-wasip2 --release` — compiles the generated code to a WebAssembly component.
4. `mpk create ...` — packages DDL, descriptors, and the WASM file into `target/wasm32-wasip2/release/wallet.mpk`.

## 4. Start the server and install the package

Start `mudud` in one terminal:

```bash
ulimit -n 65535
mudud
```

In another terminal, install the package:

```bash
mcli --http-addr 127.0.0.1:8300 app-install \
  --mpk target/wasm32-wasip2/release/wallet.mpk
```

Verify the installation:

```bash
mcli --http-addr 127.0.0.1:8300 app-list
mcli --http-addr 127.0.0.1:8300 app-detail --app wallet
```

## 5. Invoke the procedure

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
SELECT user_id, balance FROM wallets WHERE user_id IN (1001, 1002);
\q
```

## Next steps

- Read [`concepts.md`](concepts.md) for the terminology used above.
- Try the [`example/key-value`](../example/key-value/README.md) example for a smaller project that uses the key/value API.
- Write your own procedure by creating a new project similar to `example/wallet` or by modifying an existing example.
