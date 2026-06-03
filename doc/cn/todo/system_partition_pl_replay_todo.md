# System Partition 与系统表 PL Replay 设计 TODO

## 背景

当前系统中，用户数据的恢复主要依赖 WAL replay。已有两类语义日志：

- `XLBatch`：事务级逻辑日志，用于 relation / KV 的逻辑写入恢复。
- `PLBatch`：物理页级日志，用于文件/page 级变更恢复。

为了让 catalog、topology、package metadata、HA membership 等系统状态也能通过统一恢复机制重建，需要引入一个 system 级 partition。

该 system partition 内部只存放系统表。系统表的物化状态通过现有 PL replay 机制恢复。

## 目标

引入一个固定的 system partition：

```text
SYSTEM_PARTITION_ID
```

该 partition 只包含系统内部表，不存放用户表数据。

系统表包括：

- schema catalog
- partition rule catalog
- table partition binding catalog
- partition placement catalog
- node catalog
- worker endpoint catalog
- HA group catalog
- HA group member catalog
- package metadata catalog

所有系统表的文件级变更都生成 `PLBatch`，并通过 PL replay 恢复。

## 非目标

本设计不要求 PL 本身提供一致性或共识能力。

PL 只负责：

- 描述物理变更
- 按已提交顺序 replay
- 重建系统表物化状态

跨节点复制顺序、quorum commit、leader election 仍由后续 HA/Raft 层负责。

也就是说：

```text
Raft / HA log 决定顺序和提交
PLBatch 描述要 apply 的系统表物理变更
PL replay 重建本地 system partition 状态
```

不要把“直接异步复制 PL 文件”当作 HA。

## System Partition

新增固定 system partition id：

```rust
pub const SYSTEM_PARTITION_ID: OID = ...;
```

要求：

- system partition 在每个 HA 副本节点上都存在。
- system partition 不参与普通用户 partition routing。
- system partition 只允许 kernel/meta/HA 管理路径写入。
- 用户 SQL 不能直接写 system partition 的底层文件。

system partition 可以有自己的物理目录：

```text
data/
  system/
    partition-{SYSTEM_PARTITION_ID}/
```

或复用现有 relation path 结构，但必须能从 `PLFileId.partition_id == SYSTEM_PARTITION_ID` 明确识别。

## 系统表

system partition 内的系统表建议分配固定 table id。

示例：

```text
SYSTEM_TABLE_SCHEMA
SYSTEM_TABLE_PARTITION_RULE
SYSTEM_TABLE_TABLE_PARTITION_BINDING
SYSTEM_TABLE_PARTITION_PLACEMENT
SYSTEM_TABLE_NODE
SYSTEM_TABLE_WORKER_ENDPOINT
SYSTEM_TABLE_HA_GROUP
SYSTEM_TABLE_HA_GROUP_MEMBER
SYSTEM_TABLE_PACKAGE
```

每张系统表仍然是 relation/file 的形式，只是它们全部位于 system partition。

系统表物理身份：

```rust
PLFileId {
    partition_id: SYSTEM_PARTITION_ID,
    table_id: SYSTEM_TABLE_*,
    file_index,
}
```

## 系统表内容

### Schema Catalog

保存用户表 schema：

```text
table_id
table_name
columns
primary key
options
```

对应当前：

```text
__meta_schema_table
```

### Partition Rule Catalog

保存 partition rule：

```text
rule_id
rule_name
range definitions
```

对应当前：

```text
__meta_partition_rule
```

### Table Partition Binding Catalog

保存 table 到 partition rule 的绑定：

```text
table_id
rule_id
binding columns
```

对应当前：

```text
__meta_table_partition_binding
```

### Partition Placement Catalog

保存 partition 放置：

```text
partition_id
partition_group_id
worker_id
```

后续 HA 化后，placement 不应只表达 `partition_id -> worker_id`，还应能表达：

```text
partition_id -> partition_group_id
partition_group_id -> ha_group_id
```

### Node Catalog

保存集群节点：

```text
node_id
node_addr
status
epoch
```

### Worker Endpoint Catalog

保存 worker 端点：

```text
worker_id
node_id
worker_index
rpc_addr
rpc_port
```

这里的 port sharding 是 worker endpoint 的物理属性。

RPC 调用方仍然只使用 worker id。

### HA Group Catalog

保存 HA group：

```text
ha_group_id
epoch
state
```

### HA Group Member Catalog

保存 HA group 成员：

```text
ha_group_id
node_id
role
voting
```

注意：Raft membership change 的权威顺序仍来自 Raft config log。该系统表是物化结果，用于恢复和查询。

### Package Catalog

保存 package metadata：

```text
package_id
package_name
version
module names
wasm digest
install state
```

package binary 可以有两种方案：

- 小包：作为系统 blob 进入 system partition。
- 大包：保存 digest + object URI，由外部对象存储提供内容。

## 写入流程

系统状态变更不再直接只写本地 catalog relation。

目标流程：

```text
system command
  -> validate
  -> generate system table mutation
  -> build PLBatch
  -> append to HA/Raft log
  -> quorum committed
  -> apply PLBatch to system partition
  -> update in-memory metadata cache
```

其中 `PLBatch` 内的 `PLEntry.file.partition_id` 必须是 `SYSTEM_PARTITION_ID`。

## Management 线程

system partition 的写入应集中到 Management 线程处理。

系统表变更属于控制面操作，频率低，但语义强，需要全局顺序。普通 worker 不应直接修改 system partition。

Management 线程负责处理以下命令：

```text
CreateTable
DropTable
CreatePartitionRule
BindTablePartition
UpsertPartitionPlacement
RegisterWorkerEndpoint
UpdateHaGroup
InstallPackage
DropPackage
```

推荐写入路径：

```text
workers / clients
  -> submit management command
  -> Management thread
  -> validate
  -> append HA/Raft log or local system PL
  -> apply system PL to system partition
  -> update in-memory metadata/topology
  -> publish metadata snapshot
```

Management 线程需要保证：

- system mutation 串行化
- catalog xid / metadata epoch 单调递增
- system PL 在发布内存快照前已经 durable apply
- mutation command 可 replay / 幂等
- 失败时不发布半完成 metadata snapshot

在 HA/Raft 模式下：

```text
leader Management thread
  -> receive management command
  -> append Raft log
  -> committed
  -> apply system PL
  -> publish snapshot

follower Management apply loop
  -> receive committed log
  -> apply system PL
  -> publish snapshot
```

只有 leader 接受 system mutation；follower 只 apply committed log。

## 共享读模型

system partition 的读可以共享，但 worker 不应直接读取 system partition 物理文件作为热路径。

推荐使用不可变 metadata snapshot：

```rust
struct MetadataSnapshot {
    version: u64,
    schemas: Arc<HashMap<OID, SchemaTable>>,
    partition_rules: Arc<HashMap<OID, PartitionRuleDesc>>,
    partition_bindings: Arc<HashMap<OID, TablePartitionBinding>>,
    placements: Arc<HashMap<OID, PartitionPlacement>>,
    worker_endpoints: Arc<HashMap<OID, WorkerEndpoint>>,
    ha_groups: Arc<HashMap<OID, HaGroupRoute>>,
    packages: Arc<HashMap<OID, PackageMetadata>>,
}
```

Management 线程完成 system PL apply 后，原子发布新 snapshot：

```text
ArcSwap<MetadataSnapshot>
```

worker 读路径：

```text
load current MetadataSnapshot
use snapshot for planning/routing/execution
```

这样可以避免：

- worker 读到 system mutation 中间状态
- worker 与 Management apply 竞争 page/cache
- 执行热路径依赖 system partition 文件锁
- 控制面物理结构泄漏到执行面

单个请求建议绑定进入执行时的 metadata snapshot version，保证请求内部视图一致。

如果执行期间发现 topology/schema 版本变化，初版可以继续使用进入执行时的 snapshot；后续可根据操作类型返回 retry 或 metadata changed。

## 与现有快照机制的关系

metadata snapshot 可以复用当前数据快照机制的思想，但不建议直接复用数据 MVCC 的 tuple visibility 读路径。

可以复用的机制：

- 单调递增版本号，类似事务 xid。
- 请求进入时绑定一个 snapshot。
- 请求执行期间持续使用同一个 snapshot。
- mutation commit / system PL durable apply 后才发布新 snapshot。
- 旧 snapshot 通过 `Arc` 引用计数自然延迟释放。

不建议直接复用的部分：

- metadata 读不应在 worker 热路径中逐行做 tuple visibility 判断。
- schema / placement / topology 更需要整张视图的一致版本，而不是单行可见性。
- metadata 更新频率低，直接发布不可变 materialized view 更简单高效。
- worker 查询 routing/schema 时应读 `Arc<HashMap>` 这类结构，而不是直接读 system partition relation。

建议将数据快照和 metadata 快照作为两个不同对象放入请求上下文：

```rust
struct RequestContext {
    data_snapshot: WorkerSnapshot,
    metadata_snapshot: Arc<MetadataSnapshot>,
}
```

其中：

```text
WorkerSnapshot
  -> 用于用户数据 MVCC 可见性

MetadataSnapshot
  -> 用于 schema / partition / topology / package 的一致读视图
```

如果需要统一接口，可以抽象：

```rust
trait SnapshotView {
    fn version(&self) -> u64;
}
```

`WorkerSnapshot` 和 `MetadataSnapshot` 都可以暴露版本号，但二者的可见性语义不同。

结论：

```text
复用“请求绑定快照 + 版本递增 + commit 后发布 + 旧版本延迟释放”的机制；
不复用“按 tuple 版本逐行判断可见性”的数据读路径。
```

## 恢复流程

节点启动恢复：

```text
1. 读取本地 bootstrap config
2. 确定 local_node_id、data_dir、seed endpoints
3. 初始化 system partition storage
4. 从 HA/Raft committed log 或本地 durable log 读取 system PLBatch
5. replay system PLBatch
6. 加载 system tables
7. 构建 in-memory metadata/topology
8. 发布初始 `MetadataSnapshot`
9. 恢复用户 partition / worker runtime
```

恢复后得到：

```text
schema catalog
partition rule
partition placement
worker endpoint topology
HA group membership materialized view
package metadata
```

## 与现有 MetaMgr 的关系

当前 `MetaMgrImpl` 直接打开多个 catalog relation：

```text
schema_catalog
partition_rule_catalog
partition_binding_catalog
partition_placement_catalog
```

目标是将这些 relation 迁移到 system partition 下。

短期可以做兼容层：

```text
MetaMgr API
  -> SystemCatalog
  -> system partition relation
  -> PL-backed mutation
```

原有 catalog API 保持不变：

```rust
create_table
drop_table
create_partition_rule
bind_table_partition
upsert_partition_placements
```

但内部写入路径改为生成 system PL mutation。

## 与 Worker Registry 的关系

当前 `worker_registry.rs` 使用本地 marker 文件记录：

```text
worker.<worker_index>.<worker_id>.wid
partition.<worker_id>.<partition_id>.pid
```

引入 system partition 后：

- marker 文件仍可作为本节点 bootstrap/cache。
- 集群权威 worker endpoint 信息应来自 system partition 的 worker endpoint catalog。
- `WorkerRegistry` 应逐步从“本地 marker 权威”转为“system catalog + 本地启动配置”构建。

## 与 HA/Raft 的关系

推荐最终结构：

```text
HA/Raft Log
  -> System PLBatch
  -> Data XLBatch / PLBatch

System Partition
  -> replay System PLBatch
  -> materialized system tables

User Partitions
  -> replay Data XLBatch / PLBatch
  -> materialized user data
```

Raft 负责：

- leader election
- log order
- quorum commit
- membership change

PL 负责：

- physical mutation description
- local replay
- system table materialization

## 实施步骤

1. 定义 `SYSTEM_PARTITION_ID` 和系统表固定 `table_id`。
2. 为 system partition 建立独立 storage 初始化路径。
3. 将现有 meta catalog relation 映射到 system partition。
4. 为系统表 mutation 生成 `PLBatch`。
5. 引入 Management 线程，集中处理 system partition mutation。
6. 引入不可变 `MetadataSnapshot`，worker 通过 snapshot 共享读。
7. 实现 system PL replay 后加载 catalog 并发布初始 snapshot。
8. 将 worker endpoint / node / HA group / package metadata 加入系统表。
9. 将 `WorkerRegistry` 的集群权威来源改为 system catalog。
10. 将系统 PLBatch 接入后续 HA/Raft committed log。
11. 增加恢复测试：
   - replay schema catalog
   - replay partition placement
   - replay worker endpoint
   - replay HA group member
12. 增加崩溃恢复测试：
   - 写入系统表
   - flush PL
   - 重启
   - system catalog 完整恢复

## 验收标准

1. system partition 可以独立初始化。
2. 系统表全部位于 system partition。
3. schema catalog 可通过 system PL replay 恢复。
4. partition rule / binding / placement 可通过 system PL replay 恢复。
5. worker endpoint topology 可通过 system PL replay 恢复。
6. HA group membership materialized state 可通过 system PL replay 恢复。
7. package metadata 可通过 system PL replay 恢复。
8. `MetaMgr` 上层 API 不需要感知底层迁移。
9. 普通用户 partition routing 不会把用户数据写入 system partition。
10. 文档明确：system PL replay 是恢复机制，不是共识机制。
11. system partition mutation 只能通过 Management 线程执行。
12. worker 热路径通过 immutable metadata snapshot 读取系统状态。
13. metadata snapshot 发布发生在 system PL durable apply 之后。
