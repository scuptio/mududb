# MuduDB Documentation Index

This directory contains the MuduDB documentation. If you are new, follow the order below.

## Getting Started

| Document | Purpose |
|----------|---------|
| [`../README.md`](../README.md) | Project overview, architecture, and 5-minute Quick Start. |
| [`en/how_to_start.md`](en/how_to_start.md) / [`cn/how_to_start.cn.md`](cn/how_to_start.cn.md) | Detailed setup: install with `mudup`, one-command source setup, Dev Container, or manual setup. |
| [`en/running_mudud.md`](en/running_mudud.md) / [`cn/running_mudud.cn.md`](cn/running_mudud.cn.md) | Configure and start the `mudud` server. |
| [`en/DEPLOY.md`](en/DEPLOY.md) / [`cn/DEPLOY.md`](cn/DEPLOY.md) | One-click deployment on Ubuntu 24.04. |
| [`en/concepts.md`](en/concepts.md) / [`cn/concepts.cn.md`](cn/concepts.cn.md) | Core concepts: Mudu Procedure, OID, MPK, app/module/proc, and the toolchain. |

## Tutorials

| Document | Purpose |
|----------|---------|
| [`en/your_first_mpk.md`](en/your_first_mpk.md) / [`cn/your_first_mpk.cn.md`](cn/your_first_mpk.cn.md) | Build a minimal MPK package from the wallet example, install it, and invoke it. |
| [`../example/wallet/readme.md`](../crates/sdk/example/wallet/readme.md) | A complete Rust example: users, wallets, and transfers. |
| [`../example/wallet-as/readme.md`](../crates/sdk/example/wallet-as/readme.md) | The AssemblyScript version of the wallet example. |
| [`../example/key-value/README.md`](../crates/sdk/example/key-value/README.md) | Key/value API example. |
| [`../example/tpcc/README.md`](../crates/sdk/example/tpcc/README.md) | TPC-C benchmark example. |
| [`../example/ycsb/README.md`](../crates/sdk/example/ycsb/README.md) | YCSB benchmark example. |

## Concepts and Design

| Document | Purpose |
|----------|---------|
| [`en/innovative.md`](en/innovative.md) / [`cn/innovative.cn.md`](cn/innovative.cn.md) | What makes MuduDB different. |
| [`en/modern_hardware.md`](en/modern_hardware.md) / [`cn/modern_hardware.cn.md`](cn/modern_hardware.cn.md) | Design for modern hardware and async execution. |
| [`en/partition.md`](en/partition.md) / [`cn/partition.cn.md`](cn/partition.cn.md) | Partitioning model. |
| [`en/session.md`](en/session.md) / [`cn/session.cn.md`](cn/session.cn.md) | Session management. |
| [`en/syscall.md`](en/syscall.md) / [`cn/syscall.cn.md`](cn/syscall.cn.md) | System call interface overview. |
| [`en/abi/guest_host_abi.md`](en/abi/guest_host_abi.md) / [`cn/abi/guest_host_abi.cn.md`](cn/abi/guest_host_abi.cn.md) | Guest/host syscall ABI (MSSP v1): wire format, per-language integration, and interop verification. |

## Procedure Development

| Document | Purpose |
|----------|---------|
| [`en/procedure.md`](en/procedure.md) / [`cn/procedure.cn.md`](cn/procedure.cn.md) | Mudu Procedure specification and development guide. |
| [`../crates/tools/mudu_transpiler/readme.md`](../crates/tools/mudu_transpiler/readme.md) | Transpiler (`mtp`) usage for Rust and AssemblyScript. |

## API Reference

The [`lang.common/`](lang.common/) directory contains per-call reference documentation for the system API:

- [`mudu_query.md`](lang.common/mudu_query.md)
- [`mudu_command.md`](lang.common/mudu_command.md)
- [`mudu_batch.md`](lang.common/mudu_batch.md)
- [`mudu_get.md`](lang.common/mudu_get.md)
- [`mudu_put.md`](lang.common/mudu_put.md)
- [`mudu_range.md`](lang.common/mudu_range.md)
- [`mudu_open.md`](lang.common/mudu_open.md)
- [`mudu_close.md`](lang.common/mudu_close.md)

## Operations

| Document | Purpose |
|----------|---------|
| [`en/mcli_admin.md`](en/mcli_admin.md) / [`cn/mcli_admin.cn.md`](cn/mcli_admin.cn.md) | `mcli` HTTP management interface. |
| [`en/test_coverage.md`](en/test_coverage.md) / [`cn/test_coverage.md`](cn/test_coverage.md) | Coverage tracking setup. |

## Contracts and Formats

The [`en/contract/`](en/contract/) and [`cn/contract/`](cn/contract/) directories contain formal, versioned specifications for persistent formats, protocols, and deployment artifacts.

## Developer and Design Notes

The [`dev/`](dev/) directory contains developer-facing notes, including [`dev/sql_subset.md`](dev/sql_subset.md) — the SQL subset checked by `mgen check-sql`, its diagnostics, and build integration — [`dev/new_guest_language.md`](dev/new_guest_language.md), the step-by-step guide for adding a new guest language across all tools (`mpm-crate`, `mtp`, `mgen`, bindings, e2e) — [`dev/custom_types.md`](dev/custom_types.md), the authoring guide for user-defined record types in procedure signatures (`.wit` entities) — and [`dev/app_namespace.md`](dev/app_namespace.md), the per-app schema namespace semantics (default schema `mududb`), per-client visibility, migration notes, and v1 limits.

The [`cn/todo/`](cn/todo/) directory contains design documents and TODOs. Some are historical; check the header of each document to see whether it reflects the current implementation.
