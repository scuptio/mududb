# MuduDB

![build](../../actions/workflows/build.yaml/badge.svg)

[<img src="doc/pic/mudu_logo.svg" width="10%">](doc/en/name.md)

[汉语](README.cn.md)

---

MuduDB is a database system that makes it easier to build data-oriented applications and run their logic directly inside the database environment.

**It is currently in an early-stage, fast-moving demonstration phase.**

MuduDB explores a set of [innovative features](doc/en/innovative.md): in-database WebAssembly procedures written in general-purpose languages, a unified interactive-and-procedural execution model, AI-assisted database engineering, a modern hardware-oriented runtime built on `io_uring` and per-core workers, and one-click `.mpk` packaging for deployment.

---

## What is MuduDB

MuduDB brings application logic and data management into a unified execution environment. Instead of pulling data out to an application server, you write ordinary procedures in general-purpose languages, compile them to WebAssembly, and run them inside the database kernel—close to the data, under the same transactional and scheduling authority.

The same procedure can be invoked interactively from a client or executed as part of a server-side workflow, so ad-hoc exploration and production logic share one model.

## Key Features

- **Run Application Logic Inside the Database** — Business logic runs as WebAssembly procedures inside the database kernel, close to the data, cutting out repeated network round-trips.
- **One Codebase for Development and Production** — The same Mudu Procedure runs interactively during development (like an ORM) and is deployed as a server-side procedure, so there is no separate "dev vs. production" code path.
- **Use General-Purpose Languages** — Write procedures in Rust or AssemblyScript instead of database-specific stored-procedure languages such as PL/pgSQL.
- **Built-in ORM and Type Safety** — Query results map automatically to Rust structs via the `Entity` trait, and `sql_stmt!` / `sql_params!` macros provide compile-time SQL validation.
- **Modern Hardware-Optimized Runtime** — Per-core workers, `io_uring`-based asynchronous I/O, and lock-free hot paths are designed for lower tail latency and better multi-core scalability.
- **WebAssembly + MPK Packaging** — Procedures compile to WASM and are packaged as `.mpk` files containing schema, initial data, and descriptors for simple install-and-run deployment.
- **Microkernel + Plugin Architecture** — A minimal core engine (storage, ACID, query parsing/execution) plus a plug-in ecosystem.

## Architecture

<img src="doc/pic/architecture.svg" width="100%">

The figure above illustrates the architecture of MuduDB.

MuduDB follows a kernel-runtime architecture that brings application logic and data management into a unified execution environment.

The kernel provides the core correctness substrate, including storage, transaction processing, query execution, and execution control. Rather than exposing a broad client-driver surface, it defines a narrow [system call](doc/en/syscall.md) interface for session management and data access.

The runtime hosts user-defined procedures as WebAssembly via the [WebAssembly Component Model](https://component-model.bytecodealliance.org/). The runtime is intentionally passive: it does not introduce its own scheduler or an independent execution policy. Procedure execution is invoked and controlled by the kernel so that scheduling, correctness, and data access remain under a single authority.

At the handwritten source level, Mudu procedures are typically written in a sequential style using general-purpose languages (①). Unlike traditional database-specific stored procedure languages, this code can also be invoked through interactive client access (②). The toolchain can transform such procedures into deployable artifacts: synchronous source code can be transpiled into asynchronous generated forms (③), compiled to WebAssembly, and packaged together with related assets such as schema definitions and initial data.

At runtime, procedure invocation (④) executes close to the data within kernel-managed worker threads (⑤). System calls issued by user procedures trap into the kernel, where they run under kernel-controlled transactional and scheduling rules (⑥). This keeps computation and data access co-located and reduces cross-boundary interaction on the critical path.

Execution is organized around a per-core worker model. Each CPU core is assigned a dedicated worker thread, and I/O, networking, and user-code execution are multiplexed cooperatively within those workers. This minimizes inter-thread coordination, locking, and preemptive context switching, improving locality and reducing overhead.

## Documentation

- [Documentation index](doc/README.md)
- [How to start](doc/en/how_to_start.md)
- [Core concepts](doc/en/concepts.md)
- [Your first MPK tutorial](doc/en/your_first_mpk.md)
- [Deployment guide](doc/en/DEPLOY.md)

## License

MuduDB is licensed under the [Apache License 2.0](LICENSE).
