# mudu_sys_impl

Native implementation of the `mudu` system interface. This crate provides concrete OS and IO abstractions—filesystem, networking, process management, threading, synchronization, time, randomness, and async task scheduling—used by the rest of the MuduDB workspace. Downstream code should normally reach for it indirectly through `mudu_sys` and `mudu_sys_contract` rather than depending on it directly.

## Responsibility

- Implement the `mudu_sys_contract` traits for native (non-WASM) operating systems.
- Provide async/sync filesystem, networking, process, thread, synchronization, time, and randomness subsystems.
- Manage the async task runtime, including `tokio`-based executors, blocking task pools, and task-local context.
- Detect and expose host IO capabilities such as Linux `io_uring` availability.

## What does NOT belong here

- The trait contracts themselves live in `mudu_sys_contract`.
- The facade crate that most MuduDB crates should use is `mudu_sys`.
- Storage-level abstractions built on top of these system primitives belong in higher-level crates such as `mudu`.

## Main public entry points

- `Sys` — system handle providing access to all native subsystems.
- `default_env` and `default_sys_io_context` / `SysIoContext` — default environment and system IO context initialization.
- Subsystem handles: `SysEnvVar`, `SysOs`, `SysProcess`, `SysRandom`, `SysSync`, `SysTasks`, `SysThread`, `SysTime`.
- Public modules: `common`, `contract`, `env`, `env_var`, `fs`, `imp`, `io`, `net`, `process`, `provider`, `random`, `server`, `sync`, `sys_io_context`, `task`, `time`.
- Task/runtime helpers in `task::async_` and `task::sync`, plus `TaskContext`, `TaskID`, `TaskTrace`, and `tokio` re-export.
- Synchronization primitives: `Notifier`, `Waiter`, `unbounded_channel`, `ChannelSender`, `SyncReceiver`, and blocking eventfd helpers.
- `io_uring_available` — runtime probe for Linux `io_uring` support.
