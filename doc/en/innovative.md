# MuduDB execution model and tooling

MuduDB groups queries, computation and updates into procedures that execute inside the database. This can reduce client round trips for multi-step operations. The benefit depends on data volume, procedure overhead and transaction contention; measure it with the intended workload.

## Procedures and execution modes

Procedures compile to WebAssembly components and access the host database through system calls. Applications can also run outside the database process through a native adapter. These modes have different entry points and dependencies. See the [SDK capability table](../dev/sdk_distribution.md#language-and-execution-modes) for languages and examples.

The [procedure guide](procedure.md) uses the Rust facade to explain queries, parameter binding and transactions. Third-party libraries must support the chosen target and runtime; a native library is not necessarily usable in a Wasm guest.

## Type and SQL checks

The `Entity` trait maps query results to Rust structs. `mgen` generates entity code from DDL, and the build's `mgen check-sql` step validates supported SQL against that DDL.

These checks catch some type and query errors. Business rules, permissions, transaction conflicts and returned results still need tests. See the [supported SQL subset](../dev/sql_subset.md).

## Packaging and installation

An `.mpk` archive contains WebAssembly modules, DDL, descriptors and optional initial data. `mcli app-install` installs it in a running server. See [Your first MPK](your_first_mpk.md) for prerequisites and a complete invocation example.

## Kernel and runtime

The kernel owns storage, transactions and scheduling; the runtime hosts WebAssembly components. Per-core workers and the Linux `io_uring` path aim to reduce cross-thread sharing and I/O waits. See [runtime design](modern_hardware.md) for the mechanisms and constraints.

## AI-assisted development

The repository's [AI tools](../../ai_tool/) assist with design and code generation. Generated output must pass the same compilation, SQL checks and tests as handwritten code. These tools do not guarantee correctness and are not required to run MuduDB.
