# Replication and Raft Fault Tolerance


Current configuration follows the [configuration contract](contract/mudud_cfg_v1.md)
and [cluster specification](cluster_configuration.md): top-level
`data_plane.group`, optional `control_plane`, and `version = 1`. The old
`[replication]` / `[cluster]` tables are no longer the configuration interface.

Without `data_plane`, the server is self-contained. Each configured data group
selects Raft, stream, or a single replica without data replication. Data replication
streams and data Raft messages share the member's `replication_port`; inter-node
routing and the control plane use the separate `channel_port`.

Role terminology: **Leader / RaftFollower / RaftCandidate** for Raft protocol roles and
**Primary / StreamFollower** for stream mode. Raft voter/learner membership
is separate from the RaftFollower role. StreamFollower maps to the existing
`StreamRoleKind::Replica` and does not itself participate in Raft elections.
See [RaftFollower](terminology.md#raft-follower),
[StreamFollower](terminology.md#stream-follower), and
[Replica](terminology.md#replica) in the alphabetical glossary.

## Deployment shapes

| Shape | Configuration | Write and failure boundary |
|---|---|---|
| Self-contained node | No `data_plane` | Local WAL policy determines acknowledgement durability; no replica failover |
| Single-replica data group | One member, neither `replica.raft` nor `replica.stream` | No data replication; loss of the sole data copy is not tolerated |
| Asynchronous stream | `replica.stream`, `sync = false` | No follower acknowledgement wait; Primary loss can lose acknowledged writes |
| Synchronous stream | `replica.stream`, `sync = true` | Waits for durable StreamFollower acknowledgement; a copy holding the commit must survive |
| Raft | `replica.raft` | Majority durability and consensus commit; automatic election, writes after recovery barriers |

Use three or five voting members for ordinary Raft fault tolerance. One member
has no data redundancy; two members lose quorum when either is unavailable.
Durability also depends on the WAL policy and failure model; a deployment name
alone does not establish RPO=0 under arbitrary failures.

## How it works

**Roles and data scope**: a Replica is a copy of state, including the copy held by a Leader or Primary. The receiving roles are RaftFollower and StreamFollower; they reject business writes with a `leader_hint`. A Worker owns a data stream, not an independent Raft group.

**Stream mode**: the Primary commits to its local WAL and applies immediately. With `sync = true`, acknowledgement waits for a StreamFollower to confirm durability at the commit point. A `sync_timeout_ms` expiry means replication was not confirmed, even though the write is already durable locally. This ACK is not a Raft vote or consensus quorum.

**Raft mode**: each replication group has its own consensus instance. Worker WAL segments travel through external data streams; Raft carries registry management records and `GroupCommit` progress references. Commit admission requires quorum durability of the referenced payloads, satisfying both voter sets during joint consensus. Learners do not count toward that quorum. Business replay cannot advance beyond committed stream progress.

`StreamCommitDriver` implements the storage-facing commit contract, and `DataStreamMachine` promotes and applies committed segments. Before serving writes, the new Leader must complete recovery and the required barriers. Multiple data groups have independent consensus; “one Raft group” does not mean one group for the entire multi-group cluster.

The old `CommitVec` / `commit_sequencer` / `applier` / `heal` path has been removed. **Commit vector** remains the conceptual collection of committed stream frontiers; the status API's existing `commit_vec` field remains a per-Worker view, not the old Raft entry type.

## Configuration

### Raft (three-node Standalone example)

This is a complete configuration for `g1-node-1`; the addresses denote separate
hosts. Other nodes use the same membership, their own top-level `name` and data
directory, and ports appropriate to their host. Omitting `control_plane` selects
Standalone, which requires exactly one data group.

```toml
version = 1
name = "g1-node-1"
cluster = "demo"
channel_listen_port = 19600
topology_version = 1
mpk_path = "./mpk"
db_path = "./data"
listen_ip = "0.0.0.0"
http_listen_port = 8300
pg_listen_port = 5432
tcp_listen_port = 8000
enable_async = true
server_mode = "Tokio"
worker_threads = 1

[[data_plane.group]]
name = "grp-1"
worker_count = 1

[data_plane.group.replica.raft]
tick_ms = 50
election_timeout_ticks = 500

[[data_plane.group.replica.member]]
name = "g1-node-1"
host = "10.0.0.1"
channel_port = 19600
client_port = 5432
replication_port = 19527

[[data_plane.group.replica.member]]
name = "g1-node-2"
host = "10.0.0.2"
channel_port = 19600
client_port = 5432
replication_port = 19527

[[data_plane.group.replica.member]]
name = "g1-node-3"
host = "10.0.0.3"
channel_port = 19600
client_port = 5432
replication_port = 19527
```

### Stream (two-node variant)

Remove the third member from the example and replace the entire `replica.raft`
subtable with the following `replica.stream` subtable. Both nodes use the same
`bootstrap_primary`; only the top-level local `name` differs. There are no
per-node `primary = true/false` flags.

```toml
[data_plane.group.replica.stream]
bootstrap_primary = "g1-node-1"
sync = true
sync_timeout_ms = 10000
```

### Single replica and dedicated control plane

For a single-replica group, keep one member and declare neither `replica.raft`
nor `replica.stream`. A dedicated control plane with one data group is the Split
topology; multiple data groups with a dedicated control plane is Scale Out. Control members live in `control_plane.replica.member`, require
`control_plane.replica.raft`, and cannot also be data members. Dedicated data
nodes subscribe through a control-plane client, not a control Raft learner.

Standalone shares control/data consensus only when a data Raft instance exists. With stream or mode-less data, the control plane starts a separate Raft instance. Data replication mode does not determine control metadata replication mode.

### Validation and defaults

- Cluster configuration requires `name`, `cluster`, `channel_listen_port` and `topology_version`; `version` must be 1 (the parser defaults it to 1 if omitted).
- Nodes and groups use unique names; internal IDs are FNV-1a 64-bit hashes. Configuration does not contain numeric IDs; management APIs still use internal numeric IDs.
- Each group declares at most one mode. Neither mode requires exactly one member. Stream requires an existing member name in `bootstrap_primary` and an explicit `sync`; synchronous stream requires at least two members.
- Configuration loading/topology validation checks names, derived identities, member addresses, port collisions and placement references. The local name must occur in membership.
- A data replication node's derived ID is persisted in `<db_path>/node_id`; conflicting persisted identity rejects startup.
- Startup currently supplies fixed defaults of 64 flow-window segments, a 1 GiB snapshot threshold and a 60-second GC interval. `repl_flow_window_segments`, `snapshot_wal_threshold_bytes` and `gc_check_interval_secs` are no longer configurable `mudud.cfg` fields.

Use `server_mode = "Tokio"` for replicated data groups. Parsing the fields or
passing `uses_mududb_kernel` does not mean IOUring has the same cluster wiring.

## Snapshots and WAL GC

Every worker independently takes a business snapshot once its committed LSN passes `last snapshot + snapshot_wal_threshold_bytes` (a local Worker business snapshot; snapshot progress can differ between nodes): it flushes dirty pages and fsyncs the data files (a new persistence action on this path; a larger threshold makes it rarer), copies the partition's `relation/*.dat` files plus a dump of the in-memory kv store (original xids preserved) into `<db_path>/snapshot/worker_N/v<lsn>/`, and finally publishes a manifest (committed LSN, segment hash, per-file CRC64 list).

Snapshots enable two kinds of reclamation:

- **WAL prefix GC**: whole chunks below the watermark are physically deleted; the watermark is `min(connected subscriber ack, committed point, latest snapshot)` on the primary/leader and `min(committed point, latest snapshot)` on RaftFollowers or StreamFollowers, according to mode. A `<db_path>/*.gcfrontier` marker file lets restarts resume enumeration at the frontier instead of mistaking the log for empty. This Worker WAL frontier is not a Raft log index; Raft compaction and snapshot installation additionally require group protocol metadata and business snapshot coverage.
- **Stream-mode rejoin**: when a StreamFollower requests WAL already reclaimed by the Primary, the source offers a business snapshot. The receiver downloads and validates it, restores data and recovery metadata, and resumes streaming at the manifest's resume LSN. This restores Worker business state and WAL progress, not a Raft frontier.
- **Raft recovery**: business snapshot stream progress is distinct from the Raft snapshot's log index, term and membership configuration. The replication group coordinates consistent protocol and business recovery; an individual Worker's `committed_lsn` cannot be used directly as a Raft log index.

Writes after the snapshot point still stream normally; the source keeps writing during a transfer (the snapshot is self-sealed at the manifest point). relation_wal prefix GC is not done in this version (page-LSN stamps make its replay idempotent; its growth is independent of the worker WAL).

Current recovery boundary: generic group snapshot bundles, an installation
journal and some adapter image interfaces exist. Their presence alone does not
authorize Raft compaction or source-log deletion. Worker snapshots and stream
installation do not replace end-to-end group recovery covering catalog,
placement, transaction decisions and every stream frontier. Complete image
coverage, installation authority and GC must be established separately; historical
acceptance records do not prove that entire path.

## Online membership changes (raft mode)

The raft group's membership can be changed at runtime through the management HTTP endpoint `POST /mudu/repl/membership` (or the `mcli repl` subcommands). Requests must reach the current leader — a non-leader answers with a `leader_hint`, and `mcli` re-issues against the hinted node automatically:

```bash
mcli --http-addr <any-node>:8300 repl add-node --node-id 4 --host 10.0.0.4 --port 19527 --learner
mcli --http-addr <leader>:8300 repl promote-learner --node-id 4
mcli --http-addr <leader>:8300 repl remove-node --node-id 4
# add directly as a voter (skip the learner stage):
mcli --http-addr <leader>:8300 repl add-node --node-id 4 --host 10.0.0.4 --port 19527
```

The management `--node-id` is the internal FNV-1a 64-bit ID of the member name,
available from topology/status APIs. The `4` above is a command-argument
illustration, not the ID to assign to an arbitrary named node.

After a membership change, persisted consensus membership is authoritative.
A joining node still needs the current name-based
`data_plane.group[].replica.member` configuration, consistent derived identity,
replication addresses and intended topology. Do not reuse the old `peers`
“self plus any existing member” configuration fragment.

- Changes use joint consensus, one at a time; learners neither vote nor count toward quorum or become leaders.
- Promotion is subject to catch-up gates. After a leadership change, inspect the new leader and membership before retrying a failed operation.
- The membership endpoint does not rewrite local configuration or update every group directory, placement and control-plane topology.
- Respect topology versions and online membership protocols. Existing management code and tests do not establish every topology transition or wiped-node group recovery scenario.

## Observability

The read-only endpoint `GET /mudu/server/replication` reports this node's replication view (no capability gate; a single-node server answers `{"mode": "none"}` instead of 404):

- Common: `mode` (`"raft"` / `"stream"` / `"none"`), `node_id`, `role`, `serving` (whether this node accepts writes).
- `raft` section: `term`, `leader_node_id`, `commit_index`, `commit_vec` (per-worker committed LSN + segment hash), `voters` / `learners`, `conf_version_committed` / `conf_version_new` (unequal while a re-configuration is in progress).
- `stream` section: `epoch` (Primary authority generation, not transaction epoch), `primary_node_id`, `is_primary`, `sync`, `sync_timeout_ms`.
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
2. Promote the new Primary (on the chosen StreamFollower):
   ```bash
   mcli --http-addr <stream-follower>:8300 repl promote --fencing-cmd ./fence-old.sh
   # or, without a hook script, confirm explicitly that the old primary is fenced:
   mcli --http-addr <stream-follower>:8300 repl promote --yes
   ```
   `--fencing-cmd` runs locally and aborts the promote when it fails; the server also refuses a promote without fencing confirmation (HTTP 400). The promotion persists the new stream epoch first (`max(persisted, known) + 1`), then opens the write gate and records `stream_role`.
3. Point the remaining StreamFollowers at the new Primary (no restart needed; their sinks reconnect automatically):
   ```bash
   mcli --http-addr <stream-follower>:8300 repl follow --primary <new-primary-host:repl_port> --primary-node-id <N>
   ```
   `follow` probes the target and verifies its node id (guards against pointing at the wrong node), then the StreamFollower's stream reconnects and catches up; the overlap is truncated by CRC64 byte-compare.

**Stream epoch as the data-plane backstop** (distinct from transaction epoch): even if the old (zombie) primary comes back still believing it is primary, its epoch is lower — StreamFollowers verify the epoch of the acceptance frame and of every batch and reject anything below what they already know; a `follow` pointing at the zombie is refused with "stale primary". Transaction ids are encoded as `xid = (epoch << 32) | counter`, monotonic across primaries (same scheme as the raft term). Fencing stays an operator duty: the epoch backstop keeps zombie output out of the new cluster, but writes the zombie accepted locally during the partition are not automatically undone.

**Stored wins**: `<db_path>/stream_role` (role/epoch/primary address) takes precedence over `replica.stream.bootstrap_primary` in `mudud.cfg` on restart (same philosophy as the raft stored conf) — a promoted or re-pointed node keeps its new role across restarts instead of falling back to the static config. Do not delete this authority record to reset a live deployment; promotion and follow operations must preserve fencing and epoch monotonicity.

**Raft mode (automatic)**: no intervention needed. A new leader is elected within the election timeout and serves after self-healing. When the old leader recovers it rejoins as a RaftFollower, reconciles its uncommitted tail against committed stream progress, and catches up.

## Current limitations

- Only `server_mode = "Tokio"` supports replication; enabling it under Legacy / IOUring is a startup error.
- Stream mode has no online membership changes (raft mode supports them, see above); membership changes do not rewrite the nodes' local `mudud.cfg` (hand-sync recommended).
- Snapshot files are read fully into memory when produced and transferred (mind snapshot duration and memory on large datasets); concurrent reads on a StreamFollower while it restores are best-effort.
- relation_wal prefix GC is not done yet (replay is idempotent; the files keep growing over time).
- Stream mode accepts more than one StreamFollower (more than two data members), but write fan-out to multiple StreamFollowers is outside the verified envelope.
