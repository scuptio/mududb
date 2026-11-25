# Cluster configuration structure

Status: implemented configuration structure. The current `MuduDBCfg` parser,
`validate_topology`, default configuration template and Tokio startup mapping use
this structure. See the [configuration contract](contract/mudud_cfg_v1.md)
for the full server fields and defaults. Configuration acceptance is distinct
from backend support and end-to-end recovery guarantees: use Tokio for replicated
data groups. [中文规范](../zh-CN/cluster_configuration.zh-CN.md).

Remove the `[cluster]` wrapper: `control_plane`, `data_plane` and node/cluster
identity fields live at the top level. The configuration carries an explicit
`version = 1` (the configuration-format version). Nodes, data groups and the
cluster are identified by unique names; the configuration never carries a
numeric id. Internal numeric ids are derived from the names by a stable hash.
`topology_version` remains a runtime topology revision/fencing value, not a
configuration-format version: actual membership or placement changes follow the
existing versioning and online membership protocols.

## Structure

```text
version, name, cluster, channel_listen_port, topology_version
control_plane
  replica
    raft
    member[]
data_plane
  group[]
    name, worker_count
    replica
      raft | stream
      member[]
  placements[]
  rebalance_interval_ms
```

## Identity and hashing

Nodes and data groups are identified by a unique `name` (string); operators write
only the name, never a numeric id. The internal numeric identity is derived by a
stable, deterministic hash — no coordination is required:

| Identity | Derivation | Width |
|---|---|---|
| `node_id` | `fnv1a64(name)` over the UTF-8 bytes of the node/member `name` | u64 |
| `group_id` | `fnv1a64(group.name)` over the UTF-8 bytes of the group `name` | u64 |
| `cluster_id` | `fnv1a64(cluster)` over the UTF-8 bytes of the cluster name | u64 |

- The hash is **FNV-1a 64-bit** (offset basis `0xcbf29ce484222325`, prime
  `0x100000001b3`, no salt/seed), applied to the name's UTF-8 bytes. The hash is
  part of the contract: changing it is a breaking change.
- Every node derives the same id from the same `name` independently — no
  coordination and no bootstrap circular dependency. raft/stream/persistence keep
  using u64 ids unchanged.
- `name` is unique cluster-wide (across control members and data members) and
  differs per node; `cluster` is identical across all nodes (every node writes
  the same cluster name). Duplicate names or hash collisions are a config-load
  error. A u64 collision is negligible (birthday bound: ~2^32 names for 50%
  collision).
- The local role is derived from where the local `name` appears in the member
  lists (no longer from a numeric `node_id`).

## One syntax for single-group and multi-group deployments

Always use top-level `control_plane.replica.member` and
`data_plane.group[].replica.member`; single-group deployments do not get a
separate flat form. `member` is an array, even for a single member, using TOML
array-of-tables syntax:

```toml
# Member fields are shown in the complete example below.
[control_plane.replica.raft]

[[control_plane.replica.member]]

[[data_plane.group]]
name = "grp-1"
worker_count = 4

[data_plane.group.replica.raft]

[[data_plane.group.replica.member]]
```

This shows table paths only, not a complete runnable configuration. A single data
group has one `[[data_plane.group]]` entry; multiple groups repeat that
entry. Each group's `[[data_plane.group.replica.member]]` count determines
its configured replica count. Single-group does not mean single-replica.
Omitting `control_plane` selects Standalone (control and data roles use the same
member nodes); providing it selects the dedicated control plane (Split with one
data group, or Scale Out with several).

## Fields and ownership

| Path | Meaning |
|---|---|
| `version` | Configuration-format version (u32, currently 1) |
| `name` | This node's unique name (string, cluster-wide unique) |
| `cluster` | Cluster name (string, identical across all nodes) |
| `channel_listen_port` | Local inter-node channel listener |
| `topology_version` | Runtime topology revision/fencing value |
| `control_plane.replica.member[].name` | Unique name of a control bootstrap member |
| `data_plane.group[].name` | Unique data-group name |
| `data_plane.group[].worker_count` | Business Workers per member of this data group |
| `data_plane.group[].replica.member[].name` | Unique name of a data bootstrap member |
| `control_plane.replica.raft` | The dedicated control group's replication mode (Raft only) and its parameters |
| `data_plane.group[].replica.raft` | Data-group Raft parameters, mutually exclusive with stream |
| `data_plane.group[].replica.stream` | Data-group stream parameters, mutually exclusive with raft |
| `data_plane.placements[].group` | The data-group name a placement references |
| `data_plane.rebalance_interval_ms` | Existing rebalance interval; 0 disables scheduling |

Control members carry `name`, `host`, `channel_port`, and the existing
`client_port` field. Retaining `client_port` does not authorize business SQL on a
control node. Data members additionally carry `replication_port`, the address
previously supplied by `replication.peers[].port`. `channel_port` is for cluster
RPC/control traffic; `replication_port` is for the data replication listener.
`client_port` retains its existing pgwire/leader-hint meaning, not the custom TCP
worker port.

For this clustered form, nested `replica.member` is the sole configured source
of bootstrap membership and peer addresses. Do not duplicate it in
`replication.peers`. Mode and mode-specific parameters live in each group's `replica.raft` or
`replica.stream`; local listeners and other server settings remain outside the
topology. Tokio startup derives peer membership and mode from the local
data group. Configure local listeners consistently with the member addresses. Control nodes
use control-plane startup and do not create a business replication group or
business Workers. `worker_threads = 0` is not a control-role selector.

## Replication mode and mode-specific parameters

`replica` contains the shared `member` array and a mode subtable, with no duplicate
`mode` field. Each data group declares at most one of `raft` or `stream`. Declaring
neither, with a single member, selects the single-replica form (a one-node data
replica set with no replication mechanism); declaring neither with more than one
member is an error. An empty `[data_plane.group.replica.raft]` explicitly selects
Raft. The control group must declare `[control_plane.replica.raft]`;
`control_plane.replica.stream` is forbidden. The Raft-only rule applies to the
dedicated `control_plane` configuration section. At runtime, control metadata
still uses Raft. In Standalone with a data Raft instance, control and data attach
to the same shared group. With stream or mode-less data, no shared data Raft
exists and `ControlPlaneNode::start` creates a separate control Raft instance
over the cluster channel. Thus a stream data group does not imply a stream
control plane; co-location alone does not imply shared consensus.

| Subtable | Field | Contract |
|---|---|---|
| `replica.raft` | `tick_ms` | Optional positive integer milliseconds; retain the existing Raft tick default |
| `replica.raft` | `election_timeout_ticks` | Optional positive integer; retain the existing Raft election timeout default |
| `replica.stream` | `bootstrap_primary` | Required member `name` of this group; first boot only |
| `replica.stream` | `sync` | Required boolean; true waits for at least one StreamFollower's durable ACK, false permits acknowledgement after local durability |
| `replica.stream` | `sync_timeout_ms` | Optional positive integer milliseconds, default 10000; effective only when sync=true |

Raft has no configured Primary/StreamFollower assignment and forbids `bootstrap_primary`
and `sync`; commits require voter quorums, including both voter sets during joint
consensus. A stream ACK is not a Raft vote. Synchronous stream acknowledgement
also requires Primary local durability; timeout means replication was not confirmed,
not that the local transaction rolled back. `sync=true` requires at least one
member besides the Primary.

Persisted stream role and generation after promotion override `bootstrap_primary`;
restart cannot restore obsolete Primary/StreamFollower assignments. All nodes use the
same group configuration instead of per-node `primary=true/false` flags.

### Raft

```toml
[[data_plane.group]]
name = "grp-1"
worker_count = 4

[data_plane.group.replica.raft]
tick_ms = 50
election_timeout_ticks = 10

[[data_plane.group.replica.member]]
name = "g1-node-1"
host = "10.0.0.1"
channel_port = 19600
client_port = 5432
replication_port = 19527
```

### Stream

```toml
[[data_plane.group]]
name = "grp-1"
worker_count = 4

[data_plane.group.replica.stream]
bootstrap_primary = "g1-node-1"
sync = true
sync_timeout_ms = 10000

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
```

### Single replica

A group with no `replica.raft`/`replica.stream` subtable and a single member is a
single-replica data replica set: one node hosts the group's partitions with no
replication mechanism.

```toml
[[data_plane.group]]
name = "grp-1"
worker_count = 4

[[data_plane.group.replica.member]]
name = "g1-node-1"
host = "10.0.0.1"
channel_port = 19600
client_port = 5432
replication_port = 19527
```

Mode parameters belong in their corresponding subtable; `mode` and `member.role`
are not supported fields. Current validation rejects both modes together, invalid
member counts and an unknown bootstrap Primary. Unknown or misplaced fields are
not uniformly rejected by serde; they may be ignored. A mode-less group requires
exactly one member. Stream syntax does not establish every control-plane or
cross-group feature combination.

## Example: single data group, Standalone (no `control_plane`)

A single data group with no `control_plane` section is the Standalone topology:
the control and data share one Raft group in this Raft example. Three members are
three replicas and three control voters. Replacing
`[data_plane.group.replica.raft]` with `[data_plane.group.replica.stream]`
(with `bootstrap_primary` and `sync`) selects stream for data, with a separate
control Raft over the same member nodes.

```toml
version = 1
name = "g1-node-1"
cluster = "prod"
channel_listen_port = 19600
topology_version = 1

[data_plane]
rebalance_interval_ms = 0
placements = []

[[data_plane.group]]
name = "grp-1"
worker_count = 4

[data_plane.group.replica.raft]

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

## Example: Scale Out — 3 data groups × 3 replicas, 1 control group × 3 replicas

This is a complete topology fragment, not a complete server configuration.
Use it in each node's configuration, changing `name` and node-local
server settings as appropriate. Hosts are separate machines, so ports can repeat.
No partition placements are declared until corresponding rules/partitions exist.

```toml
version = 1
name = "g1-node-1"
cluster = "prod"
channel_listen_port = 19600
topology_version = 1

[control_plane.replica.raft]

[[control_plane.replica.member]]
name = "ctl-1"
host = "10.0.0.101"
channel_port = 19600
client_port = 5432

[[control_plane.replica.member]]
name = "ctl-2"
host = "10.0.0.102"
channel_port = 19600
client_port = 5432

[[control_plane.replica.member]]
name = "ctl-3"
host = "10.0.0.103"
channel_port = 19600
client_port = 5432

[data_plane]
rebalance_interval_ms = 0
placements = []

[[data_plane.group]]
name = "grp-1"
worker_count = 4

[data_plane.group.replica.raft]
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

[[data_plane.group]]
name = "grp-2"
worker_count = 4

[data_plane.group.replica.raft]
[[data_plane.group.replica.member]]
name = "g2-node-1"
host = "10.0.0.4"
channel_port = 19600
client_port = 5432
replication_port = 19527

[[data_plane.group.replica.member]]
name = "g2-node-2"
host = "10.0.0.5"
channel_port = 19600
client_port = 5432
replication_port = 19527

[[data_plane.group.replica.member]]
name = "g2-node-3"
host = "10.0.0.6"
channel_port = 19600
client_port = 5432
replication_port = 19527

[[data_plane.group]]
name = "grp-3"
worker_count = 4

[data_plane.group.replica.raft]
[[data_plane.group.replica.member]]
name = "g3-node-1"
host = "10.0.0.7"
channel_port = 19600
client_port = 5432
replication_port = 19527

[[data_plane.group.replica.member]]
name = "g3-node-2"
host = "10.0.0.8"
channel_port = 19600
client_port = 5432
replication_port = 19527

[[data_plane.group.replica.member]]
name = "g3-node-3"
host = "10.0.0.9"
channel_port = 19600
client_port = 5432
replication_port = 19527
```

There are four independent Raft groups and twelve configured voting replicas.

## Current startup boundaries

- Data-group `replica.raft.tick_ms` / `election_timeout_ticks` reach the data Raft configuration. Dedicated control-group fields parse, but `ControlPlaneNode::start` currently constructs `RaftCfg::new(..., 25, 4, ...)` instead of reading those control-group knobs. Their presence in TOML does not make them effective.
- A data node's actual Worker count comes from top-level `worker_threads`; 0 uses available machine parallelism, not the group's `worker_count`. Set the values explicitly to match. Control-node startup forces the actual count to 0.
- IOUring startup explicitly rejects `data_plane`; Legacy also lacks this cluster path. Current clustered deployments use Tokio.

## Validation

- `version` must be 1; unknown versions are errors, not a silent downgrade.
- In TOML, the top-level `version`, `name`, `cluster`, `channel_listen_port`
  and `topology_version` assignments must precede the first table header.
- `name` is unique cluster-wide (across control and data members); group `name`
  is unique. Duplicate names or hash collisions are errors.
- Omitting `control_plane` selects the Standalone topology, which requires
  exactly one `data_plane.group` entry; providing `control_plane` selects the
  dedicated control plane (Split with one data group, or Scale Out with
  several), whose control members are nonempty and disjoint (by name) from
  data members.
- The local `name` must occur exactly once (in a data group or the control
  plane); members are nonempty; `worker_count` is positive. Set the data-node local Worker count to match;
  topology validation alone does not establish that runtime equality.
- Validate member addresses and port ranges. Placement references must identify
  existing data groups (by name) and valid Worker slots; control nodes cannot
  own placements.
- `bootstrap_primary` must reference a member `name` of the same group.
- A data group with neither `replica.raft` nor `replica.stream` is a single
  replica and must have exactly one member.
- Bootstrap membership does not override persisted Raft membership. Editing a
  list is not an online add/remove/promote operation.
- Parser and topology validation are implemented in
  [`mudud_cfg`](../../crates/db-kernel/mudu_runtime/src/backend/mudud_cfg/mod.rs);
  the current default template is in the same module. Startup maps the fields in
  [`tokio_backend`](../../crates/db-kernel/mudu_runtime/src/backend/tokio_backend.rs).
  Future field changes must update the contract, template, fixtures and examples together.
- These rules describe the supported configuration. The serde structs do not
  uniformly deny unknown fields; an obsolete or misplaced key may be ignored
  rather than rejected. Do not use successful TOML parsing as proof that a key
  takes effect. Replicated data groups currently use the Tokio startup path.
