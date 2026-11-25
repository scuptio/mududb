# repl

Streaming replication and Raft fault tolerance.

Role names are Primary / StreamFollower in stream mode and Leader /
RaftFollower / RaftCandidate in Raft mode. Voter/learner membership is separate from the
RaftFollower protocol role; receiving a log stream does not imply stream mode.

Every cluster node runs a replication listener that multiplexes Raft traffic,
stream push/fetch and snapshots. In Raft mode, each Worker's WAL maps to a
data stream: payloads stay outside Raft, while `GroupCommit` commits stream
progress references. `StreamCommitDriver` submits commit segments;
`DataStreamMachine` promotes and applies committed segments. The write gate
opens only after Leader recovery converges. Stream mode uses the separate
Primary / StreamFollower path and explicit administrative promotion.

## Role

Sits between `backup` and `worker` in the server stack (`server -> backup ->
repl -> worker -> xcontract`, see
[ARCHITECTURE.md](../../../ARCHITECTURE.md)). Consumed by `server` and
`backup`; builds on `worker`, `xcontract`, `storage`, `wal`, `catalog`, and
`mudu_raft`. The crate root also hosts the per-worker replication registries
(worker handles, ack watermarks, subscriber sets, raft handles) shared by
these modules.

## Key modules

- `source` — node-level replication listener serving WAL streams and raft
  traffic.
- `sink` — receiving-side replication sink, one task per worker.
- `data_plane` — per-Worker data streams, commit/apply drivers and the
  data group's `tx_decision` stream.
- `log_stream` — stream registry, external durable logs, quorum durability,
  `GroupCommit` progress and apply scheduling.
- `control_plane` — catalog and transaction-epoch streams in the unified
  control replication group.
- `combined` — shared control/data group assembly.
- `raft_service` — Raft group lifecycle and the MSFM transport.
- `ctl` — replication-mode (non-raft) control for the write gate and the
  primary's synchronous commit path.
- `stream_admin` — stream-mode runtime role administration (promote/follow
  orchestration).
- `maintain` — per-worker replication maintenance: periodic WAL/staging
  prefix GC and snapshot triggers.
- `snapshot` — per-worker business snapshots and the snapshot transfer layout.
- `staging` — receiving-side staging store for streamed WAL frames.
- `seg_hash` — segment hash chain for raft-committed WAL segments.
- `wal_cursor` — monotonic frame cursor over a worker log's chunk files.
- `status` — replication observability: status registry and operational
  counters.

## Features

- `debug_trace` — enables extra debug tracing via `mudu_utils/debug_trace`.
- `ds` — pass-through for `mudu_sys/ds` (deterministic-simulation backend; a
  no-op in this repository).
