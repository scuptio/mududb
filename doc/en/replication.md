# Replication and Raft Fault Tolerance

MuduDB supports three deployment shapes: **single node** (default), **primary-replica streaming replication** (`stream`), and **Raft consensus fault tolerance** (`raft`). Adding a `[replication]` section to `mudud.cfg` enables clustering; without it the server runs as a single node with behavior identical to a replication-free deployment.

Both cluster shapes share one replication channel: replication streams and Raft messages are multiplexed over a single `listen_port` by message type — no separate port is needed.

## Deployment shapes

| Shape | Configuration | Write acknowledged when | Failover on primary loss | RPO of acknowledged commits |
|-------|---------------|--------------------------|--------------------------|------------------------------|
| Single node | no `[replication]` | local WAL durable (per `wal_sync_mode`) | — | — |
| Primary-replica (async) | `[replication.stream]`, `sync = false` | local WAL durable | manual | > 0 |
| Primary-replica (sync) | `[replication.stream]`, `sync = true` | any replica has flushed the commit point | manual | 0 (provided at least one replica survives) |
| Raft HA | `[replication.raft]`, 3 or 5 nodes | majority flush + consensus commit | automatic (election + self-heal) | 0 |

Rule of thumb: pick `stream` for read scaling / data redundancy with operator-controlled failover; pick `raft` (at least 3 nodes) for unattended automatic failover.

## How it works

**Data plane (shared by both shapes)**: each worker owns one replication stream; the primary pushes WAL segments (batched at 64 KiB / 32 frames / 200 µs) to replicas in order, with sliding-window flow control (`repl_flow_window_segments`). Replicas are read-only: write requests are rejected with a `leader_hint`, and clients reconnect to the primary automatically.

**Stream mode**: the primary commits to its local WAL and applies immediately. With `sync = true` the acknowledgement waits until any replica confirms it has flushed the commit point; on `sync_timeout_ms` expiry the client gets an error (the data is already durable locally — "not confirmed replicated" — and heals once a replica catches up).

**Raft mode**: all nodes form a single Raft group (`mudu_raft`, an in-repo fork of the TLA+-verified scupt-raft). Log entries carry only segment hashes and a commit vector `CommitVec{[(worker_id, lsn, seg_hash); W]}`, so one consensus round advances all worker streams. Two invariants deliver RPO = 0:

- Majority flush gating: a CommitVec covering a segment is only committed after a majority of replicas confirm holding that segment.
- Apply ≤ committed vector: replicas write incoming segments to a staging area and only replay what Raft has committed; the uncommitted tail never reaches storage, so truncating it on failover costs nothing.

When the leader fails, the survivors elect a new one within the election timeout; the new leader **heals before serving** — it compares hash chains per worker against the last committed CommitVec to locate divergence, fetches missing segments, catches up, writes a Barrier, and only then accepts writes. Transaction ids are encoded as `xid = (raft_term << 32) | counter`, monotonic across leaders.

## Configuration

Shared fields live in `[replication]`; the mode is chosen by which subsection is present — `[replication.stream]` or `[replication.raft]`. Exactly one must be present; neither or both is a configuration error.

### Primary-replica streaming (two-node example)

Node 1 (primary), `mudud.cfg`:

```toml
mpk_path = "./mpk"
db_path = "./data"
listen_ip = "0.0.0.0"
http_listen_port = 8300
pg_listen_port = 5432
enable_async = true
server_mode = "Tokio"

[replication]
node_id = 1                 # persisted to <db_path>/node_id on first boot; a mismatch is a startup error
listen_port = 19527         # shared replication + Raft port; must not collide with http/pg/tcp ports
repl_flow_window_segments = 64   # optional, default 64
snapshot_wal_threshold_bytes = 1073741824   # optional, default 1 GiB: committed WAL growth past the last snapshot that triggers the next one
gc_check_interval_secs = 60                # optional, default 60: WAL/staging GC evaluation interval

[[replication.peers]]       # static member list, identical on every node, must contain this node's node_id
node_id = 1
host = "10.0.0.1"
port = 19527

[[replication.peers]]
node_id = 2
host = "10.0.0.2"
port = 19527

[replication.stream]
primary = true              # required: exactly one node sets true, the rest false
sync = true                 # required: true = synchronous, false = asynchronous
sync_timeout_ms = 10000     # optional, default 10000
```

Node 2 (replica) differs only in `node_id = 2` and `primary = false`.

### Raft fault tolerance (three-node example)

Same shared part as above, with `node_id` 1/2/3 per node, and the mode subsection replaced by:

```toml
[replication.raft]
raft_tick_ms = 50              # optional, default 50; Raft state-machine tick interval
election_timeout_ticks = 500   # optional, default 500; election timeout = tick x this value (25s by default)
```

`peers` lists all three nodes. The group should have ≥ 3 members (or exactly 1 for development): **a two-node Raft group is discouraged as a steady-state deployment** (no automatic failover — majority = 2, so one loss stalls the group — and worse write availability than a single node) and only logs a startup warning; it is the sanctioned bootstrap shape for a joining node that names only itself and one existing member (stored conf wins; the cluster conf replaces it after joining).

### Validation rules

- Exactly one of the two mode subsections is present; `primary`/`sync` are required keys inside `[replication.stream]`.
- `peers` is non-empty, node ids and addresses are unique, and the list contains this node's `node_id`.
- `listen_port` must not collide with `http_listen_port` / `pg_listen_port` / `tcp_listen_port`.
- `node_id` is persisted to `<db_path>/node_id` on first boot; a configured value that disagrees with the persisted one is a startup error.
- `snapshot_wal_threshold_bytes` / `gc_check_interval_secs` must be greater than zero.

## Snapshots and WAL GC

Every worker independently takes a business snapshot once its committed LSN passes `last snapshot + snapshot_wal_threshold_bytes` (no Raft entry needed — every node snapshots the same committed point locally): it flushes dirty pages and fsyncs the data files (a new persistence action on this path; a larger threshold makes it rarer), copies the partition's `relation/*.dat` files plus a dump of the in-memory kv store (original xids preserved) into `<db_path>/snapshot/worker_N/v<lsn>/`, and finally publishes a manifest (committed LSN, segment hash, per-file CRC64 list).

Snapshots enable two kinds of reclamation:

- **WAL prefix GC**: whole chunks below the watermark are physically deleted; the watermark is `min(connected subscriber ack, committed point, latest snapshot)` on the primary/leader and `min(committed point, latest snapshot)` on followers. A `<db_path>/*.gcfrontier` marker file lets restarts resume enumeration at the frontier instead of mistaking the log for empty. Staging and raft_log are reclaimed in the same rhythm (the business snapshot point doubles as the Raft snapshot-install frontier).
- **Fast rejoin**: when a replica resubscribes from a point below the primary's GC frontier, the primary answers with a snapshot offer instead of a stream — the replica pages the snapshot files down, verifies their CRCs, replaces its data files and restores the kv store, installs the snapshot point as its Raft frontier, and resubscribes from `snapshot_lsn + 1`, skipping a full WAL replay. Disconnected replicas no longer hold back the primary's WAL reclamation.

Writes after the snapshot point still stream normally; the source keeps writing during a transfer (the snapshot is self-sealed at the manifest point). relation_wal prefix GC is not done in this version (page-LSN stamps make its replay idempotent; its growth is independent of the worker WAL).

## Online membership changes (raft mode)

The raft group's membership can be changed at runtime through the management HTTP endpoint `POST /mudu/repl/membership` (or the `mcli repl` subcommands). Requests must reach the current leader — a non-leader answers with a `leader_hint`, and `mcli` re-issues against the hinted node automatically:

```bash
mcli --http-addr <any-node>:8300 repl add-node --node-id 4 --host 10.0.0.4 --port 19527 --learner
mcli --http-addr <leader>:8300 repl promote-learner --node-id 4
mcli --http-addr <leader>:8300 repl remove-node --node-id 4
# add directly as a voter (skip the learner stage):
mcli --http-addr <leader>:8300 repl add-node --node-id 4 --host 10.0.0.4 --port 19527
```

Recommended join procedure (learner two-step):

1. On the leader, `add-node --learner` to register the new member (with its replication address); the joint consensus commit makes it effective.
2. Start the new node: its local `mudud.cfg` `peers` only needs **itself plus one existing member** (a two-peer raft config only logs a warning — it is the sanctioned bootstrap shape). The cluster's committed conf replaces the static local list (stored conf wins); the node catches up via snapshot/replication stream.
3. Once caught up, `promote-learner` turns it into a voter that participates in elections and quorum counts.

Notes:

- Changes commit through raft joint consensus, one at a time. If leadership changes mid-reconfiguration the request fails explicitly — re-issue it on the new leader, which keeps driving the joint consensus from its own state.
- A learner replicates the log but never votes, never counts toward a quorum, and is never elected.
- The address book is maintained kernel-side (the conf carries only node ids): host/port are registered on `add-node` and cleaned up after a remove.
- After an online change, consider hand-syncing every node's `mudud.cfg` `peers` (it only affects first-boot bootstrap; at runtime the committed cluster conf is authoritative).
- Election timeouts are purely local configuration (never replicated with the conf), so per-node phasing stays deterministic across membership changes.

## Observability

The read-only endpoint `GET /mudu/server/replication` reports this node's replication view (no capability gate; a single-node server answers `{"mode": "none"}` instead of 404):

- Common: `mode` (`"raft"` / `"stream"` / `"none"`), `node_id`, `role`, `serving` (whether this node accepts writes).
- `raft` section: `term`, `leader_node_id`, `commit_index`, `commit_vec` (per-worker committed LSN + segment hash), `voters` / `learners`, `conf_version_committed` / `conf_version_new` (unequal while a re-configuration is in progress).
- `stream` section: `epoch`, `primary_node_id`, `is_primary`, `sync`, `sync_timeout_ms`.
- `workers`: per worker `wal_next_lsn` (produced/applied high water), raft `chain_lsn` + `chain_seg_hash`, `staging_max_lsn`, and the latest snapshot (`committed_lsn` / `taken_at_ms` / directory bytes).
- `subscribers`: every subscriber as the leader/primary sees it — `(node_id, worker_index, stored_lsn, source_lsn, lag_lsn)`; `lag_lsn` is in LSN (frame) units, the gap between the source's high water and the subscriber's durable watermark.
- `storage`: worker WAL / staging / raft_log / snapshot directory sizes in bytes (**periodic cache**: a background thread refreshes them every 10 seconds; the endpoint only reads the cache and never scans the disk on the request path; the first refresh runs at startup).
- `counters`: `sync_write_timeouts`, `snapshots_taken`, `wal_chunks_reclaimed` / `staging_chunks_reclaimed` (prefix-GC reclaimed chunks), `snapshot_offers_served` / `snapshot_restores_completed`.

CLI: `mcli --http-addr <node>:8300 repl status` (JSON; `--table` renders sectioned text). Example (abridged):

```
mode: stream  node_id: 1  role: primary  serving: true
stream: epoch=2  primary=1  is_primary=true  sync=false  sync_timeout_ms=10000
== workers ==
worker  wal_next  chain_lsn  staging  snap_lsn  snap_bytes
0       128       -          -        64        4096
== subscribers ==
node  worker  stored_lsn  source_lsn  lag_lsn
2     0       100         127         27
== storage (bytes) ==
wal: 8192  staging: 0  raft_log: 0  snapshot: 4096  refreshed_at_ms: 1790039916903
```

Since this version, `mcli server-topology` also returns the `replication` field (role/leader/commit_vec) instead of dropping it.

## Failover

**Stream mode (manual, tooled through mcli)**:

1. **Fence the old primary (operator responsibility).** Preventing split brain (old and new primary both accepting writes) is the operator's first duty — make sure the old primary is down or isolated.
2. Promote the new primary (on the chosen replica):
   ```bash
   mcli --http-addr <replica>:8300 repl promote --fencing-cmd ./fence-old.sh
   # or, without a hook script, confirm explicitly that the old primary is fenced:
   mcli --http-addr <replica>:8300 repl promote --yes
   ```
   `--fencing-cmd` runs locally and aborts the promote when it fails; the server also refuses a promote without fencing confirmation (HTTP 400). The promotion persists the new epoch first (`max(persisted, known) + 1`), then opens the write gate and records `stream_role`.
3. Point the remaining replicas at the new primary (no restart needed; their sinks reconnect automatically):
   ```bash
   mcli --http-addr <replica>:8300 repl follow --primary <new-primary-host:repl_port> --primary-node-id <N>
   ```
   `follow` probes the target and verifies its node id (guards against pointing at the wrong node), then the replica's stream reconnects and catches up; the overlap is truncated by CRC64 byte-compare.

**Epoch as the data-plane backstop**: even if the old (zombie) primary comes back still believing it is primary, its epoch is lower — replicas verify the epoch of the acceptance frame and of every batch and reject anything below what they already know; a `follow` pointing at the zombie is refused with "stale primary". Transaction ids are encoded as `xid = (epoch << 32) | counter`, monotonic across primaries (same scheme as the raft term). Fencing stays an operator duty: the epoch backstop keeps zombie output out of the new cluster, but writes the zombie accepted locally during the partition are not automatically undone.

**Stored wins**: `<db_path>/stream_role` (role/epoch/primary address) takes precedence over the `primary` key of `mudud.cfg` on restart (same philosophy as the raft stored conf) — a promoted or re-pointed node keeps its new role across restarts instead of falling back to the static config. To force a node back to its configured role, delete `stream_role` and restart it.

**Raft mode (automatic)**: no intervention needed. A new leader is elected within the election timeout and serves after self-healing. When the old leader recovers it rejoins as a follower, truncates its uncommitted divergent tail against the CommitVec, and catches up.

## Current limitations

- Only `server_mode = "Tokio"` supports replication; enabling it under Legacy / IOUring is a startup error.
- Stream mode has no online membership changes (raft mode supports them, see above); membership changes do not rewrite the nodes' local `mudud.cfg` (hand-sync recommended).
- Snapshot files are read fully into memory when produced and transferred (mind snapshot duration and memory on large datasets); concurrent reads on a replica while it restores are best-effort.
- relation_wal prefix GC is not done yet (replay is idempotent; the files keep growing over time).
- Stream mode accepts more than one replica (peers > 2), but write fan-out to multiple replicas is outside the verified envelope.

For the design rationale and the deferred-work list see the [Replication + Raft design TODO (中文)](../dev/todo/replication_raft_todo.md).
