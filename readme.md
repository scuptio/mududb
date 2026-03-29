# MuduDB

![build](../../actions/workflows/build.yaml/badge.svg)

[<img src="doc/pic/mudu_logo.svg" width="10%">](doc/en/name.md)

[汉语](readme.cn.md)

---

MuduDB is a database system that makes it easier to build data-oriented applications and run their logic directly inside the database environment.

**It is currently in an early-stage, fast-moving demonstration phase.**

MuduDB explores a set of [innovative features](doc/en/innovative.md) that combine database execution, modern tooling, and AI-assisted development to improve developer efficiency and system resource utilization.


---

## Architecture

<img src="doc/pic/architecture.svg" width="100%">


The figure above illustrates the architecture of MuduDB.

MuduDB follows a kernel-runtime architecture that brings application logic and data management into a unified execution environment.

The kernel provides the core correctness substrate, including storage, transaction processing, query execution, and execution control. Rather than exposing a broad client-driver surface, it defines a narrow [system call](doc/en/syscall.md) interface for session management and data access.

The runtime hosts user-defined procedures as WebAssembly via the [WebAssembly Component Model](https://component-model.bytecodealliance.org/). The runtime is intentionally passive: it does not introduce its own scheduler or an independent execution policy. Procedure execution is invoked and controlled by the kernel so that scheduling, correctness, and data access remain under a single authority.

At the handwritten source level, Mudu procedures are typically written in a sequential style using general-purpose languages (①). Unlike traditional database-specific stored procedure languages, this code can also be invoked through interactive client access (②). The toolchain can transform such procedures into deployable artifacts: synchronous source code can be transpiled into asynchronous generated forms (③), compiled to WebAssembly, and packaged together with related assets such as schema definitions and initial data.

At runtime, procedure invocation (④) executes close to the data within kernel-managed worker threads (⑤). System calls issued by user procedures trap into the kernel, where they run under kernel-controlled transactional and scheduling rules (⑥). This keeps computation and data access co-located and reduces cross-boundary interaction on the critical path.

Execution is organized around a per-core worker model. Each CPU core is assigned a dedicated worker thread, and I/O, networking, and user-code execution are multiplexed cooperatively within those workers. This minimizes inter-thread coordination, locking, and preemptive context switching, improving locality and reducing overhead.


---

## [How to start](doc/en/how_to_start.md)

---
