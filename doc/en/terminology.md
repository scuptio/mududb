# MuduDB Terminology

[中文版](../zh-CN/terminology.zh-CN.md)

This English document is the canonical terminology reference for MuduDB documentation and design discussions. The Chinese version maps every term to the same English entry. Definitions are specific to MuduDB, not claims of industry-wide usage.

## Reading the architecture

**Partitions organize data; Workers execute within a node; replica sets own replication and failover.** Partitions exist in Standalone, Split and Scale Out deployments alike.

```text
Data replica set G1: one Raft group, four logical partitions, three replicas each

Node A (Leader)       Node B (RaftFollower)     Node C (RaftFollower)
  W0 -> P0 replica      W0 -> P0 replica      W0 -> P0 replica
  W1 -> P1 replica      W1 -> P1 replica      W1 -> P1 replica
  W2 -> P2 replica      W2 -> P2 replica      W2 -> P2 replica
  W3 -> P3 replica      W3 -> P3 replica      W3 -> P3 replica
```

This example has four partitions, twelve replicas and twelve Worker instances, but one Raft Group. Worker labels are local slots. The current main bootstrap path selects the first assigned partition for each Worker; the list-shaped identity and adoption hooks do not establish complete multi-active-partition support. Do not encode the current one-active-partition restriction into permanent terminology.

Describe deployment along separate axes:

| Axis | Vocabulary |
|---|---|
| Deployment topology | Standalone / Split / Scale Out |
| Replica and failure configuration | Voter count, learners, failure domains, HA |
| Replication mechanism | Stream replication / Raft replication |

The deployment topology combines the data replica set count and the control-plane layout: Standalone is one data replica set with an embedded control plane; Split is one data replica set with a dedicated control plane; Scale Out is multiple data replica sets with a dedicated control plane. Standalone and Scale Out are the principal topologies; Split is the intermediate topology between them (control plane already dedicated, data not yet sharded). These axes do not imply that every combination is supported. Current configuration uses top-level `data_plane` and optional `control_plane`: without `data_plane` the node is self-contained; with `data_plane` but no `control_plane`, Standalone requires exactly one data group. Dedicated data nodes use a control-plane client. See the [configuration contract](contract/mudud_cfg_v1.md) for fields, mode constraints and backend limits.

Organization follows the following rules: Entries are alphabetized by canonical English name, without entry numbers, and referenced by name and stable anchor. 
The Chinese version follows the same order; abbreviation entries point to the full definition with “See”.

## Term index

| English | 中文 |
|---|---|
| [Checkpoint](#checkpoint) | 检查点 |
| [Class](#class) | 类 |
| [Cluster](#cluster) | 集群 |
| [Combined Deployment](#combined-deployment) | 控制数据合设部署 |
| [Commit Vector](#commit-vector) | 提交向量 |
| [Control Plane](#control-plane) | 控制面 |
| [Control Plane Client (CPC)](#control-plane-client) | 控制面客户端 |
| [Control Plane Node (CPN)](#control-plane-node) | 控制面节点 |
| [Control Replica Set (CRS)](#control-replica-set) | 控制副本集 |
| [Coordinator](#coordinator) | 事务协调者 |
| [CPC](#cpc) | CPC |
| [CPN](#cpn) | CPN |
| [Cross Partition Transaction](#cross-partition-transaction) | 跨分区事务 |
| [Cross Replica Set Transaction](#cross-replica-set-transaction) | 跨副本集事务 |
| [CRS](#crs) | CRS |
| [Data Plane](#data-plane) | 数据面 |
| [Data Plane Group (DPG)](#data-plane-group) | 数据面组 |
| [Data Plane Node (DPN)](#data-plane-node) | 数据面节点 |
| [Data Replica Set (DRS)](#data-replica-set) | 数据副本集 |
| [Dedicated Control Plane](#dedicated-control-plane) | 独立控制面 |
| [DPG](#dpg) | DPG |
| [DPN](#dpn) | DPN |
| [DRS](#drs) | DRS |
| [Fencing](#fencing) | 旧权威隔离 |
| [Generation](#generation) | 代际 |
| [HA](#ha) | HA |
| [High Availability (HA)](#high-availability) | 高可用 |
| [Host](#host) | 主机 |
| [Intra Replica Set Cross Partition Transaction](#intra-replica-set-cross-partition-transaction) | 同副本集跨分区事务 |
| [Leader](#leader) | 领导者 |
| [Learner](#learner) | 学习成员 |
| [Log Sequence Number (LSN)](#lsn) | 日志序列号 |
| [Log Stream](#log-stream) | 日志流 |
| [LSN](#lsn-abbreviation) | LSN |
| [Member](#member) | 成员 |
| [Multi Replica Set Deployment](#multi-replica-set-deployment) | 多副本集部署 |
| [Node](#node) | 节点 |
| [Node Group](#node-group) | 节点组 |
| [Not Replicated](#not-replicated) | 不复制 |
| [Object](#object) | 对象 |
| [Participant](#participant) | 事务参与者 |
| [Partition](#partition) | 分区 |
| [Partition Key](#partition-key) | 分区键 |
| [Partition Merge](#partition-merge) | 分区合并 |
| [Partition Move](#partition-move) | 分区迁移 |
| [Partition Rule](#partition-rule) | 分区规则 |
| [Partition Split](#partition-split) | 分区分裂 |
| [Partitioning](#partitioning) | 分区划分 |
| [Physical Relation](#physical-relation) | 物理关系 |
| [Placement](#placement) | 分区放置 |
| [Primary](#primary) | 主节点 |
| [Quorum](#quorum) | 法定人数 |
| [Raft Group](#raft-group) | Raft 共识组 |
| [RaftCandidate](#raft-candidate) | Raft 候选者 |
| [RaftFollower](#raft-follower) | Raft 跟随者 |
| [Rebalance](#rebalance) | 负载重平衡 |
| [Replica](#replica) | 副本 |
| [Replica Set](#replica-set) | 副本集 |
| [Scale Out](#scale-out) | 横向扩容 |
| [Single Node Deployment](#single-node-deployment) | 单节点部署 |
| [Single Partition Transaction](#single-partition-transaction) | 单分区事务 |
| [Single Replica Set Deployment](#single-replica-set-deployment) | 单副本集部署 |
| [Single Replica Set HA](#single-replica-set-ha) | 单副本集高可用 |
| [Snapshot](#snapshot) | 快照 |
| [Split](#split) | 分离式 |
| [Stable Epoch](#stable-epoch) | 稳定纪元 |
| [Standalone](#standalone) | 一体式 |
| [Stream Epoch](#stream-epoch) | 流复制纪元 |
| [StreamFollower](#stream-follower) | 流复制跟随者 |
| [Struct](#struct) | 结构体 |
| [Table Binding](#table-binding) | 表分区绑定 |
| [Term](#term) | 共识任期 |
| [Transaction Epoch](#epoch) | 事务纪元 |
| [Voter](#voter) | 投票成员 |
| [Worker](#worker) | 执行器 |
| [Worker Slot](#worker-slot) | 执行器槽位 |

## Definitions

<a id="checkpoint"></a>

### Checkpoint

Chinese: **检查点**.

Persisted recovery state or progress. Name its scope and durability authority; a local file is not automatically quorum-committed control state.

<a id="class"></a>

### Class

Chinese: **类**.

The definition-side type of an object declared in DDL: a durable table, a message queue, a volatile cache, or a transient type declaration. A Class defines the shape (fields, types, nullability, keys); an [Object](#object) is an instance of a Class. The shared `ClassKind` tag names the carrier.

<a id="cluster"></a>

### Cluster

Chinese: **集群**.

A managed MuduDB deployment with a common identity and routing domain. A cluster can contain one or multiple data replica sets.

<a id="combined-deployment"></a>

### Combined Deployment

See [Standalone](#standalone).

<a id="commit-vector"></a>

### Commit Vector

Chinese: **提交向量**.

The authoritative collection of committed stream frontiers. GroupCommit carries progress references; stream payloads remain in external logs. A vector commit is not automatically a cross-stream business transaction.

<a id="control-plane"></a>

### Control Plane

Chinese: **控制面**.

The services responsible for authoritative catalog, directory, placement and epoch management. Moving complete persistent split/merge/rebalance job scheduling here remains unfinished.

<a id="control-plane-client"></a>

### Control Plane Client (CPC)

Chinese: **控制面客户端**.

A data node's subscription client to the control replica set for authoritative catalog, placement and epoch metadata. It is not a consensus member: unlike a Learner it replicates no Raft log, cannot vote or campaign, and does not count as a failure-domain replica.

<a id="control-plane-node"></a>

### Control Plane Node (CPN)

Chinese: **控制面节点**.

A node assigned control-plane duties. A dedicated CPN does not serve business partitions; current role labels alone do not prove that deployment isolation is implemented.

<a id="control-replica-set"></a>

### Control Replica Set (CRS)

Chinese: **控制副本集**.

The replica set carrying authoritative control metadata and epoch records. In the current Standalone Raft path, control and data share the underlying replication group; stream or mode-less data does not eliminate control Raft.

<a id="coordinator"></a>

### Coordinator

Chinese: **事务协调者**.

The role driving a transaction’s decision protocol. Durable authority is identified by CoordinatorRef and the coordinator data replica set, not by a transient Leader address.

<a id="cpc"></a>

### CPC

See [Control Plane Client (CPC)](#control-plane-client).

<a id="cpn"></a>

### CPN

See [Control Plane Node (CPN)](#control-plane-node).

<a id="cross-partition-transaction"></a>

### Cross Partition Transaction

Chinese: **跨分区事务**.

A transaction accessing multiple logical partitions. They may be on different Workers in the same node/replica-set; cross-partition does not necessarily mean cross-replica-set.

<a id="cross-replica-set-transaction"></a>

### Cross Replica Set Transaction

Chinese: **跨副本集事务**.

A transaction accessing partitions owned by multiple data replica sets. Its decision belongs to its coordinator’s data replica set.

<a id="crs"></a>

### CRS

See [Control Replica Set (CRS)](#control-replica-set).

<a id="data-plane"></a>

### Data Plane

Chinese: **数据面**.

The services executing business transactions, storing partition data and replicating data and coordinator decisions.

<a id="data-plane-group"></a>

### Data Plane Group (DPG)

Chinese: **数据面组**.

The series of data replica sets hosting business data, corresponding to the
config's `data_plane.group` list, composed of multiple data replica sets (DRS).
Each data replica set is single-replica (one member, no
`replica.raft`/`replica.stream`) or multi-replica (several members with a
replication mode). Use DPG for the whole series and Data Replica Set for a
single unit.

<a id="data-plane-node"></a>

### Data Plane Node (DPN)

Chinese: **数据面节点**.

A node assigned data-plane duties. In the Scale Out path, a DPN serves business partitions and subscribes to the control replica set through a control client (CPC), not a Raft learner.

<a id="data-replica-set"></a>

### Data Replica Set (DRS)

Chinese: **数据副本集**.

A replica set serving a collection of business partitions and its transaction decisions. Partitions in that replica set share replica-set-level replication and failover (including election in Raft mode); they do not each create a Raft Group.

<a id="dedicated-control-plane"></a>

### Dedicated Control Plane

Chinese: **独立控制面**.

The control-plane layout in which dedicated nodes host the control replica set, separate from the data replica set. It is the control-plane half of Split and Scale Out, and is distinct from the data replica set count; the current Scale Out path has not fully achieved this separation.

<a id="dpg"></a>

### DPG

See [Data Plane Group (DPG)](#data-plane-group).

<a id="dpn"></a>

### DPN

See [Data Plane Node (DPN)](#data-plane-node).

<a id="drs"></a>

### DRS

See [Data Replica Set (DRS)](#data-replica-set).

<a id="fencing"></a>

### Fencing

Chinese: **旧权威隔离**.

Reject operations from an obsolete owner or authority using a validated token or barrier. Merely changing a routing entry or detecting a timeout is not sufficient.

<a id="generation"></a>

### Generation

Chinese: **代际**.

A scoped incarnation or authority version, such as replica-set generation or stream generation. Always name the scope; full epoch authorization/fencing generations are not yet implemented.

<a id="ha"></a>

### HA

See [High Availability (HA)](#high-availability).

<a id="high-availability"></a>

### High Availability (HA)

Chinese: **高可用**.

The ability to continue or restore service under a specified failure model. State quorum, failure domains, recovery behavior and validation limits; a deployment label alone is not an HA guarantee.

<a id="host"></a>

### Host

Chinese: **主机**.

A physical machine or virtual machine that runs server processes. A host is a failure-domain concern, not a partition identity.

<a id="intra-replica-set-cross-partition-transaction"></a>

### Intra Replica Set Cross Partition Transaction

Chinese: **同副本集跨分区事务**.

A cross-partition transaction whose partitions all belong to one data replica set. A shared Raft Group does not remove the need for transaction coordination.

<a id="leader"></a>

### Leader

Chinese: **领导者**.

The elected coordinator of a Raft Group’s protocol and writes. Leadership is replica-set-scoped; multiple data replica sets can have different Leaders. Election alone does not prove business recovery is complete.

<a id="learner"></a>

### Learner

Chinese: **学习成员**.

A non-voting member that receives replicated state. It remains a Raft replica, not a lightweight metadata subscription client.

<a id="lsn"></a>

### Log Sequence Number (LSN)

Chinese: **日志序列号**.

A position in a specified log stream. Compare positions only within the same stream identity and incarnation; unrelated streams have no shared scalar LSN.

<a id="log-stream"></a>

### Log Stream

Chinese: **日志流**.

An independently ordered log within a replica set. Current kinds include meta, epoch, per-Worker data and tx_decision. A stream is not a partition or Raft Group.

<a id="lsn-abbreviation"></a>

### LSN

See [Log Sequence Number (LSN)](#lsn).

<a id="member"></a>

### Member

Chinese: **成员**.

A node that belongs to a replica set. Members are grouped by membership class (Voter, Learner) and occupy roles (Leader, RaftFollower, RaftCandidate in Raft mode; Primary, StreamFollower in stream mode). A node outside the replica set is not a member.

<a id="multi-replica-set-deployment"></a>

### Multi Replica Set Deployment

See [Scale Out](#scale-out).

<a id="node"></a>

### Node

Chinese: **节点**.

A server instance participating in a deployment, identified by node_id in the cluster topology. Do not equate a node with a host, partition, or Worker.

<a id="node-group"></a>

### Node Group

Chinese: **节点组**.

An existing topology/configuration term for the member nodes hosting a data replica set. Prefer Data Replica Set for ownership and Replica Set for consensus; existing ClusterGroup names are not renamed by this glossary.

<a id="not-replicated"></a>

### Not Replicated

Chinese: **不复制**.

Data or state that is not copied to another member. A data group that declares neither `replica.raft` nor `replica.stream` holds exactly one member and does not replicate its data; a control-plane client (CPC) also does not replicate the Raft log.

<a id="object"></a>

### Object

Chinese: **对象**.

The instance-side typed row: a value of a [Class](#class) — a table row, queue payload, cache value, or wire message. An Object carries the class shape plus its own values; its identity is an object identifier (OID) when one is declared.

<a id="participant"></a>

### Participant

Chinese: **事务参与者**.

A participant in transaction prepare/decision application. Qualify epoch participant separately: that denotes a member of the epoch stability evidence set, not this transaction role.

<a id="partition"></a>

### Partition

Chinese: **分区**.

A logical data-routing and placement unit identified by partition_id. Explicit partitions are defined by partition rules and retain their identity when moved; multiple tables can bind to the same rule. Tables or queues without a rule binding use the default partition (currently represented in storage by `partition_id = 0`). The default value 0 does not identify one globally placed unit shared by every unpartitioned object in the cluster; storage also depends on object identity and node context.

<a id="partition-key"></a>

### Partition Key

Chinese: **分区键**.

The value or tuple used by a partition rule to select a partition. A table binding selects the table columns supplying that value.

<a id="partition-merge"></a>

### Partition Merge

Chinese: **分区合并**.

Combine eligible partitions into a larger partition layout under a controlled transition. This differs from combining replica sets.

<a id="partition-move"></a>

### Partition Move

Chinese: **分区迁移**.

Change the placement of an existing partition while retaining its identity. Changing a Worker or data replica set does not by itself create a new logical partition.

<a id="partition-rule"></a>

### Partition Rule

Chinese: **分区规则**.

A metadata object defining key types, partition boundaries and logical partitions. The current rule kind is Range; a rule can be shared by table bindings.

<a id="partition-split"></a>

### Partition Split

Chinese: **分区分裂**.

Replace a partition’s range layout with smaller partitions under a controlled metadata transition. This changes partition boundaries and is distinct from adding replicas.

<a id="partitioning"></a>

### Partitioning

Chinese: **分区划分**.

Divide data according to a partition rule. Partitioning exists in both single-replica-set and multi-replica-set deployments and does not by itself imply cross-node distribution.

<a id="physical-relation"></a>

### Physical Relation

Chinese: **物理关系**.

A table’s storage within a partition, identified in the storage model by (table_id, partition_id). This differs from the logical partition shared through a rule.

<a id="placement"></a>

### Placement

Chinese: **分区放置**.

The assignment of a partition to a data replica set and Worker Slot. The cluster route is conceptually partition_id → (group_id, worker_index); local metadata also uses worker_id.

<a id="primary"></a>

### Primary

Chinese: **主节点**.

The designated write source in stream replication. Use Leader for the Raft protocol; do not silently transfer stream failover guarantees to Raft or vice versa.

<a id="quorum"></a>

### Quorum

Chinese: **法定人数**.

The configured set-size requirement for a decision or durability proof, normally a majority of voters. A simple count of all nodes is not sufficient during joint consensus.

<a id="raft-group"></a>

### Raft Group

Chinese: **Raft 共识组**.

One independent Raft consensus instance distributed across its members. In the generic Raft path it backs a replica set; neither a log stream nor a Worker is a separate Raft Group.

<a id="raft-candidate"></a>

### RaftCandidate

Chinese: **Raft 候选者**.

The Raft protocol role campaigning to become Leader, represented by `RaftRole::Candidate`. Campaigning requires the appropriate voter membership; learners cannot campaign. Candidate is a protocol state, not a separate membership class or permission to serve writes.

<a id="raft-follower"></a>

### RaftFollower

Chinese: **Raft 跟随者**.

The Raft protocol role that is neither leading nor campaigning, represented by `RaftRole::Follower`. It follows replication from a known Leader, but can also have no known Leader during startup or election transitions. Role and membership are separate dimensions: a voter in this role may vote; a learner may also occupy this role but cannot vote or campaign. Receiving external log streams in Raft mode does not make it a StreamFollower.

<a id="rebalance"></a>

### Rebalance

Chinese: **负载重平衡**.

Redistribute partition placement according to capacity or load policy. It may follow scale-out, but can also operate without adding nodes.

<a id="replica"></a>

### Replica

Chinese: **副本**.

A local copy of a logical partition on a member node. A replica-set member is a node holding replicas, not a replica itself; when counting, say members or replicas. Use RaftFollower for the Raft role, since a Leader also holds replicas.

<a id="replica-set"></a>

### Replica Set

Chinese: **副本集**.

A set of member nodes sharing replication membership and failover. In the generic Raft path, the replica set has one consensus state and Leader and carries multiple log streams.

<a id="scale-out"></a>

### Scale Out

Chinese: **横向扩容**.

A deployment topology with multiple data replica sets and a dedicated control plane. Scale Out expands data capacity and processing capacity by adding data replica sets and redistributing partitions; a dedicated control plane is a prerequisite. Adding a replica to an existing replica set is replica expansion, not data replica set scale-out. See also [Split](#split) and [Standalone](#standalone).

<a id="single-node-deployment"></a>

### Single Node Deployment

Chinese: **单节点部署**.

A deployment with one server node. It can contain multiple Workers and partitions, but cannot tolerate loss of that node.

<a id="single-partition-transaction"></a>

### Single Partition Transaction

Chinese: **单分区事务**.

A transaction accessing one logical partition, regardless of how many replicas hold that partition.

<a id="single-replica-set-deployment"></a>

### Single Replica Set Deployment

See [Standalone](#standalone).

<a id="single-replica-set-ha"></a>

### Single Replica Set HA

Chinese: **单副本集高可用**.

A single data replica set deployed with replicas and failover sufficient for its stated failure model. For the Raft path, describe voter count and failure-domain placement explicitly.

<a id="snapshot"></a>

### Snapshot

Chinese: **快照**.

A captured state at a specified consistent cut. Qualify read snapshot, Worker snapshot or replica-set snapshot; a local checkpoint is not automatically a complete replica-set recovery image.

<a id="split"></a>

### Split

Chinese: **分离式**.

The intermediate deployment topology between Standalone and Scale Out: one data replica set with a dedicated control plane. Split isolates the control replica set from the data replica set without sharding business data; it is a supported stepping stone toward Scale Out, not a temporary-only state. See also [Dedicated Control Plane](#dedicated-control-plane).

<a id="stable-epoch"></a>

### Stable Epoch

Chinese: **稳定纪元**.

The visibility watermark exposed by the epoch oracle after the required transactions are drained. Do not equate it with current epoch or permission to reclaim all older data.

<a id="standalone"></a>

### Standalone

Chinese: **一体式**.

A deployment topology with one data replica set and an embedded control plane: the control replica set shares the data replica set. The replica set can have one or several replicas; Standalone does not mean single-node or single-partition. See also [Split](#split) and [Scale Out](#scale-out).

<a id="stream-epoch"></a>

### Stream Epoch

Chinese: **流复制纪元**.

The Primary authority generation in stream mode, incremented on promotion to reject stale replication streams from an old Primary. It corresponds to the persisted `stream_epoch` and the stream status `epoch` field. It is neither a transaction visibility timestamp nor a Raft term; generation checks do not replace fencing the old Primary before promotion.

<a id="stream-follower"></a>

### StreamFollower

Chinese: **流复制跟随者**.

The stream-mode role that receives and applies the Primary’s log and serves reads only, currently represented by `StreamRoleKind::Replica`. This role itself does not participate in Raft elections or consensus quorums and is not a Raft learner; a synchronous replication ACK is not a Raft vote. Promotion to Primary requires an explicit administrative operation and fencing of the old Primary; it does not imply Raft automatic election guarantees.

<a id="struct"></a>

### Struct

Chinese: **结构体**.

The value-type composite: the named, field-ordered composite of the type system (formerly the "record" type family). A Struct is the value layer of a [Class](#class) or of a standalone composite value; it does not carry a storage carrier.

<a id="table-binding"></a>

### Table Binding

Chinese: **表分区绑定**.

The association between a table, a partition rule, and the table columns used for routing. It does not assign the table to a host.

<a id="term"></a>

### Term

Chinese: **共识任期**.

Raft’s election term for one Raft Group. It is separate from epoch numbering and from replica-set/stream generation.

<a id="epoch"></a>

### Transaction Epoch

Chinese: **事务纪元**.

A logical timestamp used for transaction visibility and quiescence. Unqualified epoch in a transaction context means this timestamp. It is not a stream epoch, Raft term, wall-clock time, or authorization generation.

<a id="voter"></a>

### Voter

Chinese: **投票成员**.

A member eligible to vote and count toward the configured consensus quorum. During joint consensus, both voter configurations must satisfy their quorum requirements.

<a id="worker"></a>

### Worker

Chinese: **执行器**.

A node-local execution and storage owner organized around execution resources. The current main bootstrap path selects one active partition per Worker; this is an implementation restriction, not identity equivalence.

<a id="worker-slot"></a>

### Worker Slot

Chinese: **执行器槽位**.

A worker_index within a node/replica-set configuration, used for corresponding Workers and per-Worker data streams. It is not a globally stable partition identity or a globally unique worker_id.

## Writing rules

| Avoid | Use instead |
|---|---|
| Single point / single machine to mean non-Scale Out HA | Single Replica Set HA |
| Non-sharded to imply no local partitions | Standalone |
| One Node = one shard | A Node hosts Workers and replicas |
| One Worker = one partition as a permanent identity rule | One active partition per Worker in the current main path |
| One partition = one Raft Group | Multiple partitions share a data replica set |
| Shard = partition + replica set | Partition and replicas |
| Node Group / data replica set / Raft Group used interchangeably | Identify membership, data ownership, or consensus explicitly |
| Cross-partition = cross-node = cross-replica-set | State the actual transaction scope |
| Epoch = term = generation | State the timestamp, election term, or scoped incarnation |
| Unqualified epoch | Transaction Epoch, stable epoch or stream epoch |
| “Snapshot complete” without scope | Name the captured state, cut and recovery guarantees |

Use **Partition** exclusively for the data partition concept; Shard is retired as both a concept and an alias. Use **Partitioning** for dividing data and **Cross Replica Set Partition Distribution** for distributing partitions across replica sets.

Name roles explicitly as **Leader / RaftFollower / RaftCandidate** for Raft protocol roles and **Primary / StreamFollower** in stream mode. Avoid unqualified Follower across modes; scope roles to a replica set or mode rather than treating them as global node identities. Existing type names, configuration keys and persistent fields remain unchanged until an explicit migration; legacy `shard` identifiers do not define another architecture concept. A queue partition index remains distinct from a stable `partition_id`; a rename must not conflate them.

Qualify **epoch** as transaction, stable or stream; a bare epoch is ambiguous across the timestamp, watermark and authority-generation senses.

## Implementation references

- [Worker ownership and bootstrap](../../crates/db-kernel/worker/src/worker/mod.rs)
- [Partition rules](../../crates/db-kernel/catalog/src/contract/partition_rule.rs) and [table bindings](../../crates/db-kernel/catalog/src/contract/partition_rule_binding.rs)
- [Topology and roles](../../crates/db-kernel/catalog/src/contract/cluster_topology.rs)
- [Standalone replica set assembly](../../crates/db-kernel/repl/src/combined.rs) and [server startup](../../crates/db-kernel/server/src/server/serve/mod.rs)
- [Generic stream contracts](../../crates/db-kernel/repl/src/log_stream/adapter.rs) and [data stream mapping](../../crates/db-kernel/repl/src/data_plane/mod.rs)

See also [partitioning](partition.md) and [replication](replication.md). Older design notes may use historical terms or describe planned behavior; this glossary supplies names, not a replacement implementation-status audit.
