# MuduDB Documentation Index

Choose a path for your task. Setup and tutorials describe commands to run; design notes explain implementation decisions.

| Task | Read in this order |
|---|---|
| Run an example | [Setup](en/how_to_start.md) / [环境配置](zh-CN/how_to_start.zh-CN.md) → [First MPK](en/your_first_mpk.md) / [第一个 MPK](zh-CN/your_first_mpk.zh-CN.md) |
| Develop procedures | [Concepts](en/concepts.md) / [核心概念](zh-CN/concepts.zh-CN.md) → [Procedures](en/procedure.md) / [过程开发](zh-CN/procedure.zh-CN.md) |
| Build or publish SDKs | [SDK build and language support](en/sdk_distribution.md) → [中文发布指南](zh-CN/sdk_publish_github_actions.zh-CN.md) |
| Change internals | [Architecture](../ARCHITECTURE.md) → [Contributor guide](../AGENTS.md) → the relevant design or format specification |

## Getting Started

| Document | Purpose |
|----------|---------|
| [`../README.md`](../README.md) | Project overview, architecture, and task-based documentation links. |
| [`en/how_to_start.md`](en/how_to_start.md) / [`zh-CN/how_to_start.zh-CN.md`](zh-CN/how_to_start.zh-CN.md) | Detailed setup: one-command source setup, Dev Container, or manual setup (`mudup` installer is planned). |
| [`en/running_mudud.md`](en/running_mudud.md) / [`zh-CN/running_mudud.zh-CN.md`](zh-CN/running_mudud.zh-CN.md) | Configure and start the `mudud` server. |
| [`en/DEPLOY.md`](en/DEPLOY.md) / [`zh-CN/DEPLOY.md`](zh-CN/DEPLOY.md) | Local build and smoke-test scripts for Ubuntu 24.04. |
| [`en/concepts.md`](en/concepts.md) / [`zh-CN/concepts.zh-CN.md`](zh-CN/concepts.zh-CN.md) | Core concepts: Mudu Procedure, OID, MPK, app/module/proc, and the toolchain. |

## Tutorials

| Document | Purpose |
|----------|---------|
| [`en/your_first_mpk.md`](en/your_first_mpk.md) / [`zh-CN/your_first_mpk.zh-CN.md`](zh-CN/your_first_mpk.zh-CN.md) | Build a minimal MPK package from the wallet example, install it, and invoke it. |
| [`../example/mpk/wallet/readme.md`](../example/mpk/wallet/readme.md) | A complete Rust example: users, wallets, and transfers. |
| [`../example/mpk/wallet-as/readme.md`](../example/mpk/wallet-as/readme.md) | The AssemblyScript version of the wallet example. |
| [`../example/mpk/key-value/README.md`](../example/mpk/key-value/README.md) | Key/value API example. |
| [`../example/mpk/tpcc/README.md`](../example/mpk/tpcc/README.md) | TPC-C benchmark example. |
| [`../example/mpk/ycsb/README.md`](../example/mpk/ycsb/README.md) | YCSB benchmark example. |

## Concepts and Design

| Document | Purpose |
|----------|---------|
| [`en/cluster_configuration.md`](en/cluster_configuration.md) / [`中文规范`](zh-CN/cluster_configuration.zh-CN.md) | Implemented name-based control_plane / data_plane configuration, version 1; parser validation, topology mapping and deployment constraints. |
| [`en/terminology.md`](en/terminology.md) / [`zh-CN/terminology.zh-CN.md`](zh-CN/terminology.zh-CN.md) | Alphabetical English glossary with a matching Chinese counterpart, unnumbered definitions, stable term anchors, and abbreviation cross-references: deployments, partitions, Workers, replication groups, and recovery. |
| [`en/innovative.md`](en/innovative.md) / [`zh-CN/innovative.zh-CN.md`](zh-CN/innovative.zh-CN.md) | Procedure execution model and tooling. |
| [`en/modern_hardware.md`](en/modern_hardware.md) / [`zh-CN/modern_hardware.zh-CN.md`](zh-CN/modern_hardware.zh-CN.md) | Design for modern hardware and async execution. |
| [`en/partition.md`](en/partition.md) / [`zh-CN/partition.zh-CN.md`](zh-CN/partition.zh-CN.md) | Partitioning model. |
| [`en/replication.md`](en/replication.md) / [`zh-CN/replication.zh-CN.md`](zh-CN/replication.zh-CN.md) | Primary-replica streaming replication and Raft fault tolerance: deployment shapes, configuration, and failover semantics. |
| [`en/session.md`](en/session.md) / [`zh-CN/session.zh-CN.md`](zh-CN/session.zh-CN.md) | Session management. |
| [`en/syscall.md`](en/syscall.md) / [`zh-CN/syscall.zh-CN.md`](zh-CN/syscall.zh-CN.md) | System call interface overview. |
| [`en/websocket_gateway.md`](en/websocket_gateway.md) | WebSocket gateway (`/ws`): procedure invocation and zone-scoped queue receive fan-out. |
| [`en/abi/guest_host_abi.md`](en/abi/guest_host_abi.md) / [`zh-CN/abi/guest_host_abi.zh-CN.md`](zh-CN/abi/guest_host_abi.zh-CN.md) | Guest/host syscall ABI (MSSP v1): wire format, per-language integration, and interop verification. |
| [`en/mudup.md`](en/mudup.md) | Design proposal (planned, not yet implemented; English only): `mudup` toolchain installer and version manager. |

## Procedure Development

| Document | Purpose |
|----------|---------|
| [`en/procedure.md`](en/procedure.md) / [`zh-CN/procedure.zh-CN.md`](zh-CN/procedure.zh-CN.md) | Mudu Procedure specification and development guide. |
| [`../crates/tools/mudu_transpiler/readme.md`](../crates/tools/mudu_transpiler/readme.md) | Transpiler (`mtp`) usage for Rust and AssemblyScript. |
| [`en/mpm_tool.md`](en/mpm_tool.md) | `mpm-tool`: inspect, convert and sign `.mpk` packages, plus `mpm-build` / `mpm-install` reference. |

## API Signature Stubs

The [`lang.common/`](lang.common/) directory contains auto-generated per-call signature stubs for the system API. Each stub is a `<!--quote_begin-->` block injected from source by the quote-injection workflow (see [markdown_quote](en/markdown_quote.md)) and quoted by other documents; these are not full API reference pages.

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
| [`en/mcli.md`](en/mcli.md) | `mcli` client: data-plane commands (`command`, `shell`, `put`, `get`, `range`, `invoke`, `app-invoke`) and JSON input. |
| [`en/mcli_admin.md`](en/mcli_admin.md) / [`zh-CN/mcli_admin.zh-CN.md`](zh-CN/mcli_admin.zh-CN.md) | `mcli` HTTP management interface. |
| [`en/http_api.md`](en/http_api.md) | Raw HTTP management endpoint reference (method + path). |
| [`en/test_coverage.md`](en/test_coverage.md) / [`zh-CN/test_coverage.md`](zh-CN/test_coverage.md) | Coverage tracking setup. |

## Contracts and Formats

The [`en/contract/`](en/contract/) and [`zh-CN/contract/`](zh-CN/contract/) directories contain formal, versioned specifications for persistent formats, protocols, and deployment artifacts.

## Developer and Design Notes

| Developer reference | Purpose |
|---|---|
| [SQL subset](en/sql_subset.md) | SQL supported by `mgen check-sql` and its diagnostics |
| [Binding API surface](en/binding_api_surface.md) | Module names and per-language interfaces |
| [Adding a guest language](en/new_guest_language.md) | Toolchain, codecs and integration tests |
| [Custom types](en/custom_types.md) | Records in procedure signatures |
| [Application namespaces](en/app_namespace.md) | Schema visibility and limits |
| [Markdown quote blocks](en/markdown_quote.md) | How `<!--quote_begin-->` blocks inject source snippets into the docs and how to refresh them |
| [Filesystem design (中文)](zh-CN/filesystem.zh-CN.md) | **仅中文版（Chinese only — no English version yet）.** Object storage, transaction semantics and implementation notes |
| [Queue mechanism design (中文)](zh-CN/queue.zh-CN.md) | **仅中文版（Chinese only — no English version yet）.** Queue messaging: data model, competing vs fan-out consumers, DLQ, WAIT/activation, and completion status |

For wire compatibility, use the versioned contract and ABI documents above. Design notes may include alternatives and future work; they are not installation instructions.
