# MuduDB Documentation Index

Choose a path for your task. Setup and tutorials describe commands to run; design notes explain implementation decisions.

| Task | Read in this order |
|---|---|
| Run an example | [Setup](en/how_to_start.md) / [环境配置](cn/how_to_start.cn.md) → [First MPK](en/your_first_mpk.md) / [第一个 MPK](cn/your_first_mpk.cn.md) |
| Develop procedures | [Concepts](en/concepts.md) / [核心概念](cn/concepts.cn.md) → [Procedures](en/procedure.md) / [过程开发](cn/procedure.cn.md) |
| Build or publish SDKs | [SDK build and language support](dev/sdk_distribution.md) → [中文发布指南](dev/sdk_publish_github_actions.zh-CN.md) |
| Change internals | [Architecture](../ARCHITECTURE.md) → [Contributor guide](../AGENTS.md) → the relevant design or format specification |

## Getting Started

| Document | Purpose |
|----------|---------|
| [`../README.md`](../README.md) | Project overview, architecture, and task-based documentation links. |
| [`en/how_to_start.md`](en/how_to_start.md) / [`cn/how_to_start.cn.md`](cn/how_to_start.cn.md) | Detailed setup: one-command source setup, Dev Container, or manual setup (`mudup` installer is planned). |
| [`en/running_mudud.md`](en/running_mudud.md) / [`cn/running_mudud.cn.md`](cn/running_mudud.cn.md) | Configure and start the `mudud` server. |
| [`en/DEPLOY.md`](en/DEPLOY.md) / [`cn/DEPLOY.md`](cn/DEPLOY.md) | Local build and smoke-test scripts for Ubuntu 24.04. |
| [`en/concepts.md`](en/concepts.md) / [`cn/concepts.cn.md`](cn/concepts.cn.md) | Core concepts: Mudu Procedure, OID, MPK, app/module/proc, and the toolchain. |

## Tutorials

| Document | Purpose |
|----------|---------|
| [`en/your_first_mpk.md`](en/your_first_mpk.md) / [`cn/your_first_mpk.cn.md`](cn/your_first_mpk.cn.md) | Build a minimal MPK package from the wallet example, install it, and invoke it. |
| [`../example/mpk/wallet/readme.md`](../example/mpk/wallet/readme.md) | A complete Rust example: users, wallets, and transfers. |
| [`../example/mpk/wallet-as/readme.md`](../example/mpk/wallet-as/readme.md) | The AssemblyScript version of the wallet example. |
| [`../example/mpk/key-value/README.md`](../example/mpk/key-value/README.md) | Key/value API example. |
| [`../example/mpk/tpcc/README.md`](../example/mpk/tpcc/README.md) | TPC-C benchmark example. |
| [`../example/mpk/ycsb/README.md`](../example/mpk/ycsb/README.md) | YCSB benchmark example. |

## Concepts and Design

| Document | Purpose |
|----------|---------|
| [`en/innovative.md`](en/innovative.md) / [`cn/innovative.cn.md`](cn/innovative.cn.md) | Procedure execution model and tooling. |
| [`en/modern_hardware.md`](en/modern_hardware.md) / [`cn/modern_hardware.cn.md`](cn/modern_hardware.cn.md) | Design for modern hardware and async execution. |
| [`en/partition.md`](en/partition.md) / [`cn/partition.cn.md`](cn/partition.cn.md) | Partitioning model. |
| [`en/replication.md`](en/replication.md) / [`cn/replication.cn.md`](cn/replication.cn.md) | Primary-replica streaming replication and Raft fault tolerance: deployment shapes, configuration, and failover semantics. |
| [`en/session.md`](en/session.md) / [`cn/session.cn.md`](cn/session.cn.md) | Session management. |
| [`en/syscall.md`](en/syscall.md) / [`cn/syscall.cn.md`](cn/syscall.cn.md) | System call interface overview. |
| [`en/abi/guest_host_abi.md`](en/abi/guest_host_abi.md) / [`cn/abi/guest_host_abi.cn.md`](cn/abi/guest_host_abi.cn.md) | Guest/host syscall ABI (MSSP v1): wire format, per-language integration, and interop verification. |
| [`en/mudup.md`](en/mudup.md) | Design proposal (planned, not yet implemented; English only): `mudup` toolchain installer and version manager. |

## Procedure Development

| Document | Purpose |
|----------|---------|
| [`en/procedure.md`](en/procedure.md) / [`cn/procedure.cn.md`](cn/procedure.cn.md) | Mudu Procedure specification and development guide. |
| [`../crates/tools/mudu_transpiler/readme.md`](../crates/tools/mudu_transpiler/readme.md) | Transpiler (`mtp`) usage for Rust and AssemblyScript. |

## API Signature Stubs

The [`lang.common/`](lang.common/) directory contains auto-generated per-call signature stubs for the system API. Each stub is a `<!--quote_begin-->` block injected from source by the quote-injection workflow (see [markdown_quote](dev/markdown_quote.md)) and quoted by other documents; these are not full API reference pages.

- [`mudu_query.md`](lang.common/mudu_query.md)
- [`mudu_command.md`](lang.common/mudu_command.md)
- [`mudu_batch.md`](lang.common/mudu_batch.md)
- [`mudu_get.md`](lang.common/mudu_get.md)
- [`mudu_put.md`](lang.common/mudu_put.md)
- [`mudu_range.md`](lang.common/mudu_range.md)
- [`mudu_open.md`](lang.common/mudu_open.md)
- [`mudu_close.md`](lang.common/mudu_close.md)
- [`proc_key_traits.md`](lang.common/proc_key_traits.md)
- [`transfer_funds.md`](lang.common/transfer_funds.md)

## Operations

| Document | Purpose |
|----------|---------|
| [`en/mcli_admin.md`](en/mcli_admin.md) / [`cn/mcli_admin.cn.md`](cn/mcli_admin.cn.md) | `mcli` HTTP management interface. |
| [`en/test_coverage.md`](en/test_coverage.md) / [`cn/test_coverage.md`](cn/test_coverage.md) | Coverage tracking setup. |

## Contracts and Formats

The [`en/contract/`](en/contract/) and [`cn/contract/`](cn/contract/) directories contain formal, versioned specifications for persistent formats, protocols, and deployment artifacts.

## Developer and Design Notes

| Developer reference | Purpose |
|---|---|
| [SQL subset](dev/sql_subset.md) | SQL supported by `mgen check-sql` and its diagnostics |
| [Binding API surface](dev/binding_api_surface.md) | Module names and per-language interfaces |
| [Adding a guest language](dev/new_guest_language.md) | Toolchain, codecs and integration tests |
| [Custom types](dev/custom_types.md) | Records in procedure signatures |
| [Application namespaces](dev/app_namespace.md) | Schema visibility and limits |
| [Markdown quote blocks](dev/markdown_quote.md) | How `<!--quote_begin-->` blocks inject source snippets into the docs and how to refresh them |
| [Page size migration TODO](dev/todo/page_size_migration_todo.md) | Pending work item (待实现 / not yet implemented): page-size migration tool |
| [Replication + Raft TODO (中文)](dev/todo/replication_raft_todo.md) | Design and delivery checklist: v1 (stream / raft modes) delivered and documented in [en/replication.md](en/replication.md); deferred items include snapshot transfer, online membership changes, and WAL GC |
| [Filesystem design (中文)](cn/filesystem.cn.md) | **仅中文版（Chinese only — no English version yet）.** Object storage, transaction semantics and implementation notes |

For wire compatibility, use the versioned contract and ABI documents above. Design notes may include alternatives and future work; they are not installation instructions.
