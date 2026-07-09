# Wallet Example

A minimal MuduDB application that demonstrates stored procedures written in Rust.
It models a simple wallet system with users, wallets, transactions, and orders,
and shows how to package them into a `.mpk` file for deployment.

## What it demonstrates

- Writing Mudu Procedures in Rust with the `/**mudu-proc**/` marker.
- Using `mudu_query` and `mudu_command` from `sys_interface::sync_api` inside procedures.
- Generating entity bindings and async procedure wrappers with `mgen` and `mtp`.
- Building a `wasm32-wasip2` component and packaging it into a `.mpk` file with `mpm-build`.
- Installing and invoking the package via `mcli`.

## Procedures

| Procedure | Description |
|-----------|-------------|
| `create_user` | Create a user and an empty wallet. |
| `deposit` | Deposit funds into a wallet. |
| `withdraw` | Withdraw funds from a wallet. |
| `transfer_funds` | Transfer funds between two wallets. |
| `balance` | Read the balance of a wallet. |

## Prerequisites

Follow the setup in [`doc/en/how_to_start.md`](../../doc/en/how_to_start.md) or run the one-click setup script from the repository root:

```bash
bash script/shell/install_deps.sh
```

Make sure these tools are in `PATH`:

- `cargo` and the `wasm32-wasip2` target
- `cargo-make`
- `mgen`, `mtp`, `mpm-build`, `mudud`, `mcli`

You can install the workspace binaries with:

```bash
python script/build/install_binaries.py
```

## Build the `.mpk` package

From this directory:

```bash
cargo make package
```

This will:

1. Reinstall the workspace CLI tools (`mgen`, `mtp`, `mpm-build`, etc.) from source so the toolchain stays in sync with the current commit.
2. Run `mgen` to generate Rust entity types from `sql/ddl.sql`.
3. Run `mtp` to transpile the Rust procedures into async generated wrappers.
4. Format the generated sources with `cargo fmt`.
5. Build the Rust code to a `wasm32-wasip2` component.
6. Run `mpm-build create` to produce `target/wasm32-wasip2/release/wallet.mpk`.

## Run it

### 1. Start `mudud`

```bash
ulimit -n 65535
mudud
```

### 2. Install the wallet package

```bash
mcli --http-addr 127.0.0.1:8300 app-install \
  --mpk ../../target/wasm32-wasip2/release/wallet.mpk
```

### 3. Create users

```bash
mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke \
  --app wallet --module wallet --proc create_user \
  --json '{"user_id": 1001, "name": "Alice", "email": "alice@example.com"}'

mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke \
  --app wallet --module wallet --proc create_user \
  --json '{"user_id": 1002, "name": "Bob", "email": "bob@example.com"}'
```

### 4. Deposit and transfer

```bash
mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke \
  --app wallet --module wallet --proc deposit \
  --json '{"user_id": 1001, "amount": 5000}'

mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke \
  --app wallet --module wallet --proc transfer_funds \
  --json '{"from_user_id": 1001, "to_user_id": 1002, "amount": 1200}'
```

### 5. Verify balances

```bash
mcli --addr 127.0.0.1:9527 shell --app wallet
```

In the shell:

```sql
SELECT user_id, balance FROM wallets WHERE user_id IN (1001, 1002);
\q
```

## Files of interest

- `src/rust/procedures.rs` — the handwritten Rust procedures.
- `src/generated/procedures.rs` — the transpiler-generated async wrappers.
- `sql/ddl.sql` — the application schema.
- `Makefile.toml` — the build pipeline.
- `package/package.cfg.json` — package metadata for `mpm-build create`.

For the AssemblyScript version of the same example, see `example/wallet-as`.
