# MuduDB Innovative Features

MuduDB rethinks the boundary between application and database. Instead of treating the database as a passive storage tier, MuduDB lets developers write ordinary procedures in general-purpose languages, run them inside the database kernel, and deploy them as versioned packages. The result is a system that combines the ergonomics of modern application development with the performance and correctness characteristics of co-located execution.

---

## 1. AI-Assisted Database Engineering

MuduDB uses large language models to accelerate database engineering. It can generate ER diagrams, DDL scripts, and procedure code from high-level descriptions. Because the generated code is written in typed, general-purpose languages, it goes through compile-time validation before it ever reaches the server, reducing the risk of runtime errors in AI-generated logic.

**Highlights**

- Generate ER diagrams from natural-language requirements
- Generate DDL scripts and table definitions
- Generate stored procedures and helper functions
- Validate AI-generated code at compile time

## 2. Mudu Procedure (MP)

[Mudu Procedure](procedure.md) is the central programming model of MuduDB. A single function written in Rust or AssemblyScript can be invoked interactively during development and then deployed as a server-side procedure in production. The transpiler takes synchronous-looking source code and produces the async wrappers that the WebAssembly runtime needs, so developers do not have to manage async state machines by hand.

**Highlights**

- One codebase for interactive development and production deployment
- Procedures run close to the data inside the database kernel
- Synchronous developer experience with async runtime execution
- Supports both ad-hoc transactions and structured workflows

## 3. Modern Hardware-Optimized Architecture

MuduDB is designed for modern multi-core servers and NVMe storage. It uses Linux [io_uring](modern_hardware.md) as the unified async I/O model, assigns one worker thread to each CPU core, and keeps connections, queues, and state local to that worker whenever possible. User procedures are written in a sequential style but execute as continuations resumed by io_uring completion events.

**Highlights**

- Per-core workers with explicit ownership boundaries
- Unified network and file I/O through `io_uring`
- Lock-free or low-lock structures on the hot path
- Continuation-driven execution without blocking worker threads

## 4. Microkernel Architecture Design

MuduDB keeps the core engine small and focused on essentials: storage, ACID transactions, query parsing, and query execution. Everything else lives in a plugin ecosystem that can evolve independently. This separation makes the kernel easier to reason about and test, while still leaving room for extensions such as JSON/graph support, external runtime modules, and custom storage engines.

**Highlights**

- Small, focused core engine
- Plugin ecosystem for extensions and custom runtimes
- Custom storage engines can be added without kernel changes
- Clearer ownership and a smaller trusted computing base

## 5. Write Procedures in General-Purpose Languages

MuduDB procedures are ordinary functions in Rust or AssemblyScript, marked with a `/**mudu-proc**/` directive. Developers keep their existing editors, debuggers, test frameworks, and package managers. There is no proprietary stored-procedure language to learn, and no vendor lock-in.

**Highlights**

- Rust and AssemblyScript support today
- Use familiar tooling: Cargo, crates, language servers, debuggers
- Reusable libraries and standard version-control workflows
- No need to learn PL/pgSQL or T-SQL

## 6. Built-in ORM and Type Safety

MuduDB provides first-class support for mapping query results to language types. The `Entity` trait turns SQL rows into Rust structs, while `sql_stmt!` and `sql_params!` macros catch SQL and parameter errors at compile time. The `mgen` tool can generate entity types directly from DDL, reducing boilerplate without adding a heavy external ORM.

**Highlights**

- Automatic relation-to-object mapping via the `Entity` trait
- Compile-time SQL validation with `sql_stmt!` and `sql_params!`
- `mgen` generates typed entities from DDL
- Early detection of schema and query mismatches

## 7. WebAssembly + MPK Packaging

Procedures compile to WebAssembly components and are packaged into `.mpk` files. An MPK is a self-contained archive that bundles DDL, optional initial data, procedure descriptors, and one or more WASM modules. Installing an app is a single `mcli app-install` command, which makes deployments reproducible and versioned.

**Highlights**

- Compile procedures to portable WebAssembly components
- Package schema, data, and logic into one `.mpk` file
- Install or upgrade apps with a single CLI command
- Versioned deployments that keep schema and code in sync

## 8. Data Proximity Processing

Because MuduDB procedures run inside the database kernel, data transformations happen next to the data rather than across the network. A procedure can query multiple rows, compute results, and update state within a single transactional boundary, without repeatedly serializing intermediate data to a remote client.

**Highlights**

- Business logic executes close to the data
- Fewer network round-trips for multi-step operations
- Simpler transaction semantics for complex workflows
- Higher throughput for data-intensive tasks

---

## At a Glance

| Dimension | Traditional Approach | MuduDB |
|---|---|---|
| Where business logic runs | Application server | Inside the database kernel |
| Stored-procedure language | PL/pgSQL, T-SQL, etc. | Rust, AssemblyScript |
| Development vs. production | Different code or patterns | Same Mudu Procedure code |
| AI-generated code validation | Mostly runtime | Compile-time type checking |
| I/O and concurrency model | Thread pool + blocking I/O | `io_uring` + per-core workers |
| Deployment artifact | SQL migrations + app binaries | Single `.mpk` package |
