# mudu_sys_contract

Defines the system-level async I/O contracts and performance-tracing primitives used by `mudu_sys` implementations to abstract filesystem, network, task scheduling and instrumentation across different runtime backends.

## Responsibility

- Trait definitions for async files, filesystems, networks, listeners, streams, task scheduling and provider composition.
- Shared system types such as provider identifiers.
- Lightweight, thread-local performance tracing primitives (transaction stages, trace contexts, spans and snapshots).
- Providing a backend-neutral interface that `mudu_sys_impl`, `mudu_sys_wasm` and higher-level crates can implement against.

## What does NOT belong here

- Concrete OS/runtime implementations (e.g. Tokio/io_uring backends) belong in [`mudu_sys_impl`](../mudu_sys_impl) and [`mudu_sys_wasm`](../mudu_sys_wasm).
- The target-selecting facade crate is [`mudu_sys`](../mudu_sys).
- Database engine logic, storage formats and SQL processing belong in crates such as [`mudu`](../mudu), [`mudu_kernel`](../mudu_kernel) and [`sql_parser`](../sql_parser).

## Main public entry points

- `mudu_sys_contract::common` — shared system types, including `ProviderType`.
- `mudu_sys_contract::contract` — async I/O traits and provider abstractions:
  - `AsyncFile`, `AsyncFs`, `AsyncNet`, `AsyncStream`, `AsyncListener`
  - `AsyncIoProvider`, `IoProviderBase`
  - `SysTaskAsync`, `FileOptions`, `AsyncMode`
- `mudu_sys_contract::perf` — performance tracing primitives:
  - `TxnStage`, `TraceContext`, `PerfSpan`, `PerfSnapshot`
  - `LocalCollector`, `StageBucket`
  - `set_enabled`, `set_sample_rate`, `snapshot`, `should_sample`
