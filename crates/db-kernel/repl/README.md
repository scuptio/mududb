# repl

Primary-replica streaming replication with Raft fault tolerance.

Every cluster node runs a node-level replication listener that streams worker
WAL frames to subscribed followers and multiplexes raft traffic; each follower
worker stages streamed frames and promotes raft-committed frames into its WAL;
the leader's commit path is quorum-gated by a per-worker commit sequencer, and
leader-election healing converges followers before client writes resume.

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
- `sink` — follower-side replication sink, one task per worker.
- `applier` — promote applier: moves raft-committed frames from staging into
  the worker WAL.
- `raft_service` — raft group lifecycle and the MSFM transport for raft
  traffic.
- `commit_sequencer` — per-worker commit sequencer on the raft leader.
- `heal` — leader-elect heal: converges every worker to the last
  raft-committed vector before client writes are ungated.
- `ctl` — replication-mode (non-raft) control for the write gate and the
  primary's synchronous commit path.
- `stream_admin` — stream-mode runtime role administration (promote/follow
  orchestration).
- `maintain` — per-worker replication maintenance: periodic WAL/staging
  prefix GC and snapshot triggers.
- `snapshot` — per-worker business snapshots and the snapshot transfer layout.
- `staging` — follower-side staging store for streamed WAL frames.
- `seg_hash` — segment hash chain for raft-committed WAL segments.
- `wal_cursor` — monotonic frame cursor over a worker log's chunk files.
- `status` — replication observability: status registry and operational
  counters.

## Features

- `debug_trace` — enables extra debug tracing via `mudu_utils/debug_trace`.
- `ds` — pass-through for `mudu_sys/ds` (deterministic-simulation backend; a
  no-op in this repository).
