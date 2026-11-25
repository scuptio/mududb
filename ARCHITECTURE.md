# MuduDB Workspace Architecture

This document describes the crate layout, dependency direction, and key design decisions for the `mududb` workspace.

Use the [English terminology reference](doc/en/terminology.md) for architecture
terms; the [Chinese counterpart](doc/zh-CN/terminology.zh-CN.md) links each term to its
canonical English definition. Both glossaries follow English alphabetical order;
cite terms by name and stable anchor, without entry numbers. Data topology (single-group/multi-group), control
layout (combined/dedicated), and replica configuration are separate concepts.
Use Partition exclusively for data partitions. Distinguish RaftFollower
(`RaftRole::Follower`) from StreamFollower (`StreamRoleKind::Replica`);
Raft membership (voter/learner) is separate from protocol role.

The current configuration uses top-level `control_plane.replica.member` and
`data_plane.group[].replica.member`, with `version = 1` and name-based node,
group and cluster identities (FNV-1a 64-bit internally). The parser, validation,
default template and Tokio startup mapping implement the
[configuration specification](doc/en/cluster_configuration.md).
Omitting `data_plane` selects a self-contained node. With `data_plane`, omitting
`control_plane` selects Combined and requires exactly one data group; explicit
control members select a dedicated control plane. A data group declares at most
one of `replica.raft` / `replica.stream`; neither means exactly one member with
no data replication. The control group permits only `replica.raft`.

## Repository Layout

The repository root is **not** a cargo workspace. Crates live under `crates/`
in group workspaces, each with its own `Cargo.toml` and `Cargo.lock`:

```text
crates/
├── common/      # foundation, sys abstraction, types, bindings, parser, client,
│                # CAF container codec (mudu_caf)
├── db-kernel/   # kernel, runtime, raft consensus (mudu_raft), server (mudud),
│                # adapter, ws gateway (ws_gateway), testing
├── sdk/         # mududb SDK, bindings (assemblyscript, python), APIs
├── tools/       # mudu_cli, mudu_gen, mudu_transpiler, mpm_build,
│                # mpm_crate, mpm_install, mpm_tool
└── bindings/    # standalone cdylib workspace: mudu_adapter_ffi (native adapter
                 # ABI); NOT part of any group workspace, isolated target-dir
example/
└── mpk/         # separate consumer workspace: guest-SDK examples (wallet, tpcc, ...)
```

The five group/example workspaces are `crates/{common,db-kernel,sdk,tools}` and
`example/mpk`. The guest-SDK examples were split out of `crates/sdk` into
their own workspace (now the top-level `example/mpk`) so that the SDK library
workspace no longer compiles guest wasm crates (and their kernel-scale
dev-dependencies) on every `cargo test --workspace`.

Cross-group path dependencies (e.g. `crates/db-kernel/*` depending on
`crates/common/mudu`) are legal and used heavily; cargo builds them as normal
path dependencies of the consuming workspace. All workspaces share the
repository-root `target/` directory (root `.cargo/config.toml` sets
`[build] target-dir = "target"`), so build artifacts like
`target/wasm32-wasip2/release/*.mpk` land at the repository root regardless
of which workspace produced them. Run cargo commands per
workspace (`cd crates/<group> && cargo ...`); `script/ci/check_all.py`
orchestrates fmt/clippy/test-compile across all five.

## Crate Layers

```text
┌─────────────────────────────────────────────────────────────┐
│  Apps / Examples / CLI / Tests                               │
│  (mudud, mudu_cli, mpm_build, mpm_install, testing)          │
├─────────────────────────────────────────────────────────────┤
│  Server                                                      │
│  (server -> backup -> repl -> ws_gateway ->           │
│   worker -> xcontract)                         │
├─────────────────────────────────────────────────────────────┤
│  Kernel                                                      │
│  (conn -> sql -> command -> meta │
│   -> storage -> xengine -> wal ->       │
│   index -> catalog / page_buffer)                            │
├─────────────────────────────────────────────────────────────┤
│  Runtime & Raft                                              │
│  (mudu_runtime, mudu_raft, sys_interface_standalone)         │
├─────────────────────────────────────────────────────────────┤
│  Language / Binding / Contract                               │
│  (mudu_contract, mudu_binding, mudu_type, mudu_transpiler,  │
│   sql_parser, mudu_gen, mudu_client)                        │
├─────────────────────────────────────────────────────────────┤
│  Utilities                                                   │
│  (mudu_utils, mudu_build_common)                            │
├─────────────────────────────────────────────────────────────┤
│  System Abstraction                                          │
│  (mudu_sys -> mudu_sys_impl -> mudu_sys_contract)           │
├─────────────────────────────────────────────────────────────┤
│  Foundation                                                  │
│  (mudu: result types, error codes, macros, pure utilities)  │
└─────────────────────────────────────────────────────────────┘
```

The original `mudu_kernel` monolith (76.9k lines) was split into sixteen
kernel crates plus a thin `mudu_kernel` facade. `mudu_kernel` now only
re-exports the kernel crates (and keeps `compat`, the fuzz harness, and
test utilities), preserving the historical `mudu_kernel::server`,
`mudu_kernel::storage`, etc. module paths for `mudu_runtime`, `mudu_cli`, and
`testing`.

## Key Rules

1. **`mudu` is a pure foundation crate.**
   - It provides common types (`RS`), error codes (`EC`), the `mudu_error!` macro, and pure serialization helpers.
   - It must **not** perform I/O, read environment variables, or depend on `mudu_sys`.
   - File I/O helpers (e.g. `read_json`, `write_toml`) live in `mudu_utils`.

2. **`mudu_sys_impl` is the single place for OS / async abstractions.**
   - Wrappers for `std::fs`, `std::thread`, `std::net`, `tokio`, etc.
   - It is re-exported by `mudu_sys`.
   - All other crates use `mudu_sys::*` instead of raw `std` / `tokio` APIs.

3. **No crate outside `mudu_sys_impl` may use disallowed `std`/`tokio` APIs.**
   - The workspace clippy configuration enforces this via `disallowed_methods` and `disallowed_types`.

4. **Build scripts share logic through `mudu_build_common`.**
   - Path resolution, file-copy-on-change, workspace version extraction, and tree-sitter constant generation are centralized there.

5. **Integration tests share helpers through `testing::support`.**
   - Common boilerplate such as `supports_server_mode`, `temp_dir`, `TestListener`, and `wait_until_backend_ready` should be reused rather than duplicated.

## Dependency Notes

- `page_buffer` is a rank-0 kernel leaf that depends only on foundation/system
  libraries. It owns format-independent byte-page caching, pin/latch handles,
  allocation, async backing stores and reuse epochs. `index` consumes its
  non-generic `dyn BufferManager`; B-tree page formats and checkpoint manifests
  remain above this boundary. `storage` consumes the concrete
  `ClockBufferManager` for its time-series files (their WAL-first physical
  logging stays in `storage`); `storage` must not become a dependency of
  either. Paged trees and buffer managers are thread-confined
  (`!Send`/`!Sync`). A partition's LocalSet owns page mutation and eviction;
  resident operations use checked local borrows, and suspension retains
  explicit leases/snapshots. Cross-thread callers send owned requests through
  relation mailboxes, never share a tree. Mailbox driver and per-call tasks
  spawn through `mudu_sys::task::async_::spawn_thread_confined_task`, which
  targets the io_uring worker task registry when a worker ring is installed
  and the active tokio `LocalSet` otherwise; the io_uring bootstrap
  (`drive_future_with_ring`) pumps that registry so worker startup can run
  storage operations, and registry background tasks (perpetual mailbox
  drivers) never block the shutdown drain. Shared metadata catalogs have
  their own owner executors. Index benchmarks compare local tasks on one
  thread; task count is not core count.

- `mudu_sys_impl` depends on `mudu` for `RS`/`EC`/`mudu_error!`. 
- `mudu_utils` depends on both `mudu` and `mudu_sys`, making it the natural home for higher-level I/O utilities.
- `mudu::channel` holds the channel wire protocol (envelope, message kinds, typed-message traits; pure); `mudu_utils::channel` hosts the endpoint transport trait, the endpoint registry with `spawn_endpoint`/`EndpointGuard` lifecycle, and the typed ask/tell runtime. Any thread may register an endpoint — the transports themselves (tokio/io_uring) live in `server`. Identity is single-sourced at thread granularity: a worker thread's task id = worker id = channel endpoint id (threads spawn with `spawn_thread_named_with_id`); tasks point at their hosting thread via `TaskContext::parent_thread`, so a trace line joins directly to its worker/endpoint.
- `sql_parser` parses DDL into `TableDef` and also hosts the SQL static-check core (`sql_parser::check`) used by `mgen check-sql`.
- `mududb.ds` is a separate workspace that consumes the public crates from `mududb` for deterministic simulation / model checking.

### Dependency consistency and layering enforcement

- `versions.toml` (repo root) is the single source of truth for external
  dependency specs shared across two or more group workspaces (including the
  `example/mpk` consumer workspace). `script/ci/sync_workspace_deps.py`
  rewrites the matching lines in the `crates/*/Cargo.toml`
  `[workspace.dependencies]` blocks; its `--check` mode fails on drift and also
  requires every shared dependency to be listed (and every listed dependency
  to be shared).
- `script/ci/check_arch.py` enforces the crate layering: `common` may not
  depend on `db-kernel`/`sdk`/`tools`, `db-kernel` may not depend on
  `sdk`/`tools`, and `sdk`/`tools` may not depend on each other. The pure
  `mudu` crate must depend on nothing. The `example/mpk` crates are treated as
  part of the `sdk` layer. Both scripts run in `script/ci/check_all.py` and
  the `workspace-invariants` CI job.
- `crates/bindings/mudu_adapter_ffi` is a standalone cdylib workspace (native
  adapter ABI). It is deliberately excluded from the group/example workspaces
  and redirects its build artifacts to its own `target-ffi/` so its unhashed
  `libmudu_adapter_ffi.so` never lands in the shared repository-root `target/`.


### Asynchronous replication runtime

`mudu_raft::core` remains a synchronous protocol state machine. Each group
has one serialized async driver task on a process-shared protocol executor
(one worker and at most four Tokio blocking IO threads). `store::AsyncStore`
is the injectable durability boundary; the production store uses explicitly
Tokio-backed `mudu_sys` filesystem contracts. Disk recovery, append, truncation,
rotation, fsync and compaction run asynchronously. A driver allows one store
batch at a time and releases its messages and results only after persistence.
Shutdown drains actual IO completion rather than cancelling disk futures.

Ingress reserves separate bounded control/proposal lanes and byte budgets;
proposal payload budgets remain reserved through commit or rejection. Latest
status subscriptions may coalesce, so consumers fence on term as well as role.
Ordered async commit subscriptions fail closed on overflow. Synchronous handle
and proposal-wait APIs reject use from an async executor; async equivalents
cover startup, proposals, membership, configuration reads, snapshots and shutdown.

`repl` owns business replay and observers outside the Raft owner. Metadata
replay and epoch dispatch/tickers share a dedicated current-thread executor with
a `LocalSet` for storage mailboxes, a bounded admission queue, and a four-thread
blocking IO pool. Task joins preserve replay panics as errors. Data-group status,
configuration and commit observers run on the existing replication IO executor.
The data durability gate checks both voter sets during joint consensus and
binds proposal admission to the configuration versions used for that check.
`mudu_raft::stream` defines business-independent group/stream identities and
progress references. Raft groups may additionally bootstrap a disjoint learner
set (`RaftCfg::with_learners`); learners replicate the log but never campaign
or vote.

`mudu_raft::stream` also carries versioned registry management records. The
`repl::log_stream` service bootstraps registrations directly through Raft,
replays paged committed entries into a complete registry/progress vector, and
retains request receipts and identity tombstones. Adapter kind/codec/digest
capabilities must be explicitly available at recovery. Its atomic checkpoint
contains protocol metadata only; business snapshots remain a separate
prerequisite for group compaction, data GC and snapshot installation.

Generic stream ACK rounds bind cluster/group/generation, registry version,
stream incarnation, exact LSN/digest, codec, leader term and both configurations.
`AckTracker` creates a durability certificate only after both voter majorities
are satisfied. Peer-session replacement or tracker disposal revokes certificates;
neutral `AdmissionGuard`s let the Raft driver reject revoked queued proposals.

`repl::log_stream::StreamAdapter` defines the external durable-log/business
apply boundary. `StreamApplyScheduler` validates record identity, contiguous
per-stream LSNs and explicit dependencies against committed and applied
prefixes before applying independent streams concurrently in bounded rounds.
Adapters own atomic business effects, durable applied frontiers and deduplication;
the scheduler is not a snapshot or leader readiness barrier.

`repl::log_stream::GenericReplicationGroup` binds a registry and external logs to
one existing Raft handle. `DurableStreamLog` reuses LGFM staging and the existing
versioned CRC-derived prefix checksum; stream payloads and retry identities stay
outside Raft. `StreamTransport` provides authenticated session leases whose
revocation guards are checked at protocol admission. Leader barriers and missing
frame fetch precede tail truncation and new proposals. A replica that learns a
committed progress reference but lacks the payload fetches the missing frames
from group members before applying (`catch_up_missing`, invariant C4).
`LoggedStreamAdapter` connects this log to business-owned atomic
application/deduplication transactions. A batched append covers N records with
one barrier and one GroupCommit (dedup retries of a fully committed batch skip
both). Prefix GC reclaims records below a snapshot-protected frontier: the chain
anchor is published durably before any deletion, and a peer needing reclaimed
bytes must install a business snapshot rather than fetch them. The promotion
gate (`member_caught_up`) compares each stream's committed progress against a
candidate's exact durable prefix at that LSN before a learner may become a
voter. Per-stream counters (append/apply/GC) live in `StreamMetrics` with
group/stream-scoped keys; the apply scheduler starts higher-priority streams
first within each round. The registry replays shared business Raft logs by
skipping non-registry entries, so a registry may ride a data group's Raft log.

### Data-plane streams

`repl::data_plane` maps every worker's WAL to one data stream of the node
group's replication group (G-03): a stream record is one commit segment (the
contiguous WAL frames between two group-commit watermarks). The per-worker
`StreamCommitDriver` keeps the storage-facing `CommitSequencer` contract, so
the worker commit path is unchanged while the segment commits through the
generic mechanism. `DataStreamMachine` promotes committed segments into the
worker WAL and replays them (leader: pure chain advance; RaftFollower: append +
replay), persisting the applied frontier with the WAL resume point. The
group's `tx_decision` stream carries cross-partition transaction decisions
(`TxDecisionMachine`, X-02), rebuilt from the durable log on restart.
`DataPlaneService` assembles in two phases (group before workers spawn,
machines as workers install) and opens the write gate only after the leader
converges. Stream push/fetch/progress run over the replication listener
(`ReplStreamTransport` on the sender, `source.rs` dispatch on the receiver);
the old CommitVec path (commit_sequencer/applier/heal) is removed.

### Unified control plane

`repl::control_plane::ControlPlaneService` replaces the old separate
MetaGroup/EpochGroup services: one control Raft group per cluster (members
from `control_plane.replica.member`, or the single data group's members in
Combined). Dedicated data nodes use `DataPlaneControlClient`; they do not
run control Raft learners. The control group carries registry management
records and `GroupCommit`
progress vectors. The catalog and epoch records themselves replicate through
two external log streams (`control.meta`, `control.epoch`) with a
quorum-durable gate ahead of every committed progress reference. Per-record
request ids dedup retries across redirects, failovers and restarts; a replica
serves an idempotent retry's result from its rebuilt request index instead of
re-applying.

The catalog stream applies committed `MetaOp`s to the node's catalog and
placement overlay; the record's stream LSN is the placement version. The epoch
stream applies advances to the shared cluster epoch state and checkpoints
`(stable, current)` with the applied frontier, so a restart resumes rather
than regresses the epoch. Every epoch advance remains fully persisted during
this transition (the P4 sparse-persistence protocol replaces it later).
Applied frontiers are atomic files written after the business effect.

The server wires the control plane in `server::control_plane_node`: raft
frames, the client RPC face (the `MetaGroupRpcRequest` contract is unchanged),
stream push/fetch with session revocation, and the epoch heartbeat reporter
all share one node endpoint. DDL and migration consumers
(`xcontract::meta_group::ChannelMetaGroup`, `migration_driver`, `rebalance`)
use stable idempotency keys and a term-guarded leader hint cache. The cluster
namespace is the stable hash of the top-level `cluster` name, so membership
changes do not redefine it. The control service fails closed until bootstrap
stream registration completes; clients retry through it.
Cross-group transaction decisions belong to the initiating data group's
`tx_decision` stream. `DataPlaneService` installs a `TxDecisionClient` on each
worker; `xcontract::rpc::coordinator` persists Begin/Decide records before
settling participants. This replaces the old control-catalog decision path.
The existence of these paths does not certify complete group snapshot installation,
log reclamation or arbitrary crash recovery.

Staging now uses explicit Tokio filesystem I/O. Mutation tasks retain their
async owner until the actual operation finishes, even if the waiting caller is
cancelled; server replication shutdown drains those tasks. All dirty chunks and
directory entries are synchronized before durability is acknowledged, and
committed anchors are published by fsync/rename/directory-fsync instead of an
in-place overwrite. Recovery from a failed mutation requires draining first.

The production cluster transport now bounds each peer's outbound queue by both
message count and serialized wire bytes, retaining the byte charge through the
socket write. Small consensus/epoch messages and responses have a reserved
control lane; a bounded control burst keeps data eligible for service. Oversized
or saturated submissions return explicit errors. This preserves the existing
wire format and does not yet change connection discovery or authenticate the
new generic stream session protocol.


The control-plane refactor targets a breaking cutover. Legacy API, protocol and
disk-format compatibility, fallback readers, dual writes and automatic import of
old data are not required. Unsupported storage identities/formats fail explicitly.
Synchronous Raft subscriptions and RaftStore I/O wrappers have been removed;
observers and persistence tests use the async interfaces directly. Remaining old
production services must be replaced along with their callers, not retained as
compatibility backends of the new group service.
