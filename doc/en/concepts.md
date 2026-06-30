# MuduDB Concepts

This page explains the core concepts and terms used throughout the MuduDB documentation and toolchain.

## Mudu Procedure (MP)

A Mudu Procedure is a user-defined function that runs inside the MuduDB engine, close to the data. Unlike traditional stored procedures written in database-specific languages (e.g., PL/pgSQL), Mudu Procedures are written in general-purpose languages such as Rust or AssemblyScript.

Key properties:

- Marked with the `/**mudu-proc**/` (Rust) or `/**mudu-proc*/` (AssemblyScript) comment directive.
- The first parameter is always an `OID` (see below), representing the current session / transaction context.
- Subsequent parameters and the return value are expressed in ordinary language types.
- The `mtp` transpiler can turn synchronous source code into async generated wrappers suitable for the WASM runtime.

See [procedure.md](procedure.md) for the full procedure specification.

## OID

**OID** stands for Object Identifier. In the context of procedures, the first `OID` argument is the session or transaction handle passed in by the kernel. Procedures use it when calling MuduDB system APIs such as `mudu_query`, `mudu_command`, `mudu_get`, or `mudu_put`.

You do not create or parse OIDs manually; they are provided by the runtime when a procedure is invoked.

## MPK Package

An **MPK** file (`.mpk`) is a MuduDB application package. It is a ZIP archive that contains:

- Application metadata (name, version, language).
- `ddl.sql` — schema definitions.
- `init.sql` — optional initial data.
- Procedure descriptors.
- One or more compiled WebAssembly component modules.

You create an MPK with the `mpk create` command and install it into a running `mudud` server with `mcli app-install`.

## App, Module, and Procedure

When an MPK is installed, MuduDB organizes its contents as follows:

- **App** — the top-level application name (e.g., `wallet`).
- **Module** — a WebAssembly component module inside the app (e.g., `wallet`).
- **Procedure** — an exported function within the module that can be invoked (e.g., `transfer_funds`).

To invoke a procedure:

```bash
mcli --addr 127.0.0.1:9527 app-invoke \
  --app wallet \
  --module wallet \
  --proc transfer_funds \
  --json '{"from_user_id": 1001, "to_user_id": 1002, "amount": 1200}'
```

## Toolchain

| Tool | Purpose | When you need it |
|------|---------|------------------|
| `mudud` | The MuduDB server. | Always, to run the database. |
| `mcli` | TCP protocol client and HTTP management CLI. | To run SQL interactively, install packages, and invoke procedures. |
| `mgen` | Source generator. Creates Rust entity types from SQL DDL. | When your application has SQL tables and wants typed query results. |
| `mtp` | Transpiler. Transforms Rust/AssemblyScript source into Mudu procedure format and generates async wrappers. | Whenever you write or change a `/**mudu-proc**/` function. |
| `mpk` | Package builder. Produces `.mpk` files from DDL, descriptors, and WASM modules. | Before deploying an application to `mudud`. |
| `mudup` | Release installer. Downloads and activates released binaries. | For daily use or server deployment without building from source. |

## System Call Interface

MuduDB exposes a narrow system-call-like API that procedures use to access the database. The synchronous API is in `sys_interface::sync_api`; the async API is in `sys_interface::async_api`. Common calls include:

- `mudu_query` — execute a SELECT statement and read results.
- `mudu_command` — execute an INSERT/UPDATE/DELETE statement.
- `mudu_get` / `mudu_put` / `mudu_range` — key/value API.
- `mudu_batch` — execute multiple statements as a batch (empty parameters only in `mudud`).

See the `doc/lang.common/` directory for per-call reference documentation.

## Component Model

MuduDB runs user procedures as WebAssembly components using the [WebAssembly Component Model](https://component-model.bytecodealliance.org/). Procedures are compiled to `wasm32-wasip2` and then composed with the MuduDB host component before being packaged into an MPK.

The `mudu_wasm` crate (published under the Cargo name `mod_0`) provides the generated guest bindings and host-side transpilation helpers.

## Interactive vs. Stored-Procedure Execution

The same Mudu Procedure source can run in two modes:

- **Interactive** — called directly from Rust test or benchmark code through the standalone adapter, useful for development and debugging.
- **Stored-procedure** — packaged as MPK, installed into `mudud`, and invoked through `mcli` or the TCP client.

See [procedure.md](procedure.md) for a longer discussion.
