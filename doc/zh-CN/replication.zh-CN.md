# 复制与 Raft 容错


当前配置与[配置契约](contract/mudud_cfg_v1.md)及[集群配置规范](cluster_configuration.zh-CN.md)一致：使用顶层 `data_plane.group`、可选 `control_plane` 和 `version = 1`。旧 `[replication]` / `[cluster]` 表不再是当前配置接口。

未配置 `data_plane` 时运行自包含节点；配置数据组后，每组选择 Raft、stream 或无复制的单副本模式。数据复制流与数据 Raft 消息复用成员的 `replication_port`；跨节点路由和控制面另用 `channel_port`，不要混淆两类端口。

角色统一为 **Leader / RaftFollower / RaftCandidate**（Raft 协议角色）和 **Primary / StreamFollower**（流复制模式）。Replica 指状态副本，Leader 和 Primary 也持有副本；voter/learner 是 Raft 成员资格，不是另一套从端角色。参见按英文名称排序的术语表中的 [RaftFollower](terminology.zh-CN.md#raft-follower)、[StreamFollower](terminology.zh-CN.md#stream-follower) 和 [Replica](terminology.zh-CN.md#replica)。

## 部署形态

| 形态 | 配置 | 写入与故障边界 |
|---|---|---|
| 自包含节点 | 无 `data_plane` | 本地 WAL 策略决定应答持久性，无副本故障切换 |
| 单副本数据组 | 一个 member，无 `replica.raft` / `replica.stream` | 无数据复制，不容忍唯一数据副本丢失 |
| 异步流复制 | `replica.stream`，`sync = false` | 不等待从端确认；Primary 丢失可能损失已应答写入 |
| 同步流复制 | `replica.stream`，`sync = true` | 等待 StreamFollower 持久确认；故障后必须保留持有提交的副本 |
| Raft | `replica.raft` | 多数派持久化与共识提交；自动选主，恢复屏障完成后开放写入 |

Raft 通常采用三个或五个投票成员；单成员没有数据冗余，双成员失去任一成员即失去多数派。持久性还取决于 WAL 策略和故障模型，不能仅凭部署名称承诺任意故障下 RPO=0。

## 工作原理

**角色与数据范围**：Replica 是状态副本，包括 Leader / Primary 持有的副本。接收端角色为 RaftFollower / StreamFollower；它们拒绝业务写入并返回 `leader_hint`。Worker 拥有数据流，不构成独立 Raft 组。

**流复制模式**：Primary 提交本地 WAL 后立即 apply。`sync = true` 时，应答等待 StreamFollower 确认该提交点已持久化。`sync_timeout_ms` 超时表示未确认复制，本地写入可能已持久化。此 ACK 不是 Raft 投票或共识 quorum。

**Raft 模式**：每个复制组拥有独立共识实例。Worker WAL 段经外部数据流复制，Raft 承载注册表管理记录和 `GroupCommit` 进度引用。提交准入要求对应数据满足 voter quorum 持久化；联合共识须同时满足两个 voter 集合，learner 不计入 quorum。业务回放不能越过已提交的流进度。

`StreamCommitDriver` 实现存储侧提交契约，`DataStreamMachine` 提升并应用已提交段。新 Leader 完成恢复及必要屏障后才能开放写入。多个数据组各有共识实例，“一个 Raft 组”不表示整个多组集群共用一个组。

旧 `CommitVec` / `commit_sequencer` / `applier` / `heal` 路径已移除。**提交向量**仍指各流已提交前沿的集合；状态 API 现有 `commit_vec` 字段保留为按 Worker 展示的视图，不是旧 Raft 日志条目类型。

## 配置

### Raft（三节点一体式 Standalone 示例）

以下是节点 `g1-node-1` 的完整配置，三个地址代表不同主机。其他节点使用相同成员表，修改顶层 `name` 并使用各自数据目录；端口须与部署环境一致。省略 `control_plane` 选择一体式（Standalone），要求恰好一个数据组。

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

### 流复制（双节点变体）

在上面的配置中删除第三个 member，将整个 `replica.raft` 子表替换为下列 `replica.stream` 子表。两个节点均使用相同的 `bootstrap_primary`，只修改顶层本机 `name`；不再逐节点设置 `primary = true/false`。

```toml
[data_plane.group.replica.stream]
bootstrap_primary = "g1-node-1"
sync = true
sync_timeout_ms = 10000
```

### 单副本与独立控制面

单副本数据组保留一个 member，且不声明 `replica.raft` 或 `replica.stream`。独立控制面配单数据组即分离式（Split），配多数据组即横向扩容（Scale Out）；控制成员放在 `control_plane.replica.member`，必须声明 `control_plane.replica.raft`，不能兼任数据成员。独立数据节点通过控制面客户端订阅，不运行控制 Raft learner。

一体式（Standalone）只有在已有数据 Raft 时共享控制与数据共识；stream/无数据复制时，控制面仍另外启动 Raft。数据组复制模式不决定控制元数据的复制模式。

### 校验与默认值

- 集群配置需有 `name`、`cluster`、`channel_listen_port`、`topology_version`；`version` 必须为 1（省略时解析器默认 1）。
- 节点与组使用唯一名称，内部 ID 为 FNV-1a 64-bit；配置不直接填写数字 ID。管理接口仍使用内部数字 ID。
- 每组最多一个模式子表；都不声明时必须恰好一个成员。stream 必须提供组内存在的 `bootstrap_primary` 和 `sync`；同步流复制至少两个成员。
- 名称、派生身份、成员地址、端口冲突与 placement 引用在配置加载/拓扑校验时检查；本机名称必须出现在成员表中。
- 数据复制节点的派生 node ID 持久化到 `<db_path>/node_id`；与已有值冲突时拒绝启动。
- 当前启动装配固定使用流窗口 64 段、快照阈值 1 GiB、GC 检查间隔 60 秒；`repl_flow_window_segments`、`snapshot_wal_threshold_bytes` 和 `gc_check_interval_secs` 不再是 `mudud.cfg` 可调字段。

复制数据组使用 `server_mode = "Tokio"`。字段可解析或 `uses_mududb_kernel` 返回真不表示 IOUring 已接入同样的集群装配。

## 快照与 WAL GC

每个 worker 在 committed LSN 越过 `上次快照 + snapshot_wal_threshold_bytes` 时独立做业务快照（本地 Worker 业务快照；各节点快照进度可以不同）：先 flush 脏页并 fsync 数据文件（这是新增的持久化动作，阈值越大开销越稀），再把该分区的 `relation/*.dat` 与内存 kv 存储的 dump（保留原始 xid）写入 `<db_path>/snapshot/worker_N/v<lsn>/`，最后落盘 manifest（含 committed_lsn、段哈希、文件 CRC64 清单）即发布。

快照让两类回收成为可能：

- **WAL 前缀 GC**：低于水位的整 chunk 物理删除，水位 = Primary min(在线订阅者确认水位, 已提交点, 最近快照点)、StreamFollower min(已提交点, 最近快照点)；`<db_path>/*.gcfrontier` 标记文件保证重启后从 frontier 继续，不会把日志误判为空。该 Worker WAL 水位不等于 Raft 日志索引；Raft 日志压缩和快照安装还需复制组的协议元数据及业务快照覆盖证明。
- **流复制重 join**：StreamFollower 请求的 WAL 已被 Primary 回收时，源端返回业务快照 offer。接收端下载并校验快照、恢复数据和恢复元数据，然后按 manifest 的恢复 LSN 继续订阅。此路径恢复的是 Worker 业务状态和 WAL 进度，不安装 Raft 前沿。
- **Raft 恢复**：须区分业务快照的流进度与 Raft 快照的日志索引、任期和成员配置。复制组负责协调协议状态与业务状态的一致恢复；不能把单个 Worker 的 `committed_lsn` 直接当成 Raft 日志索引。

注意：快照点之后的第一批写仍会推流补齐；快照传输期间源端继续写不受影响（快照以 manifest 点自封）。relation_wal 的前缀 GC 本版未做（其页 LSN 戳保证重放幂等，体积增长独立于 worker WAL）。

当前恢复边界：通用组快照 bundle、安装 journal，以及部分 adapter 镜像接口已经存在；它们本身不授权 Raft compaction 或源日志删除。Worker 快照与流复制安装不能代替包含 catalog、placement、事务决策和全部流前沿的端到端组级恢复。完整业务镜像覆盖、安装授权和 GC 必须分别检查，不能根据旧验收记录推断已全部闭环。

## 在线成员变更（raft 形态）

运行中可通过管理 HTTP 端点 `POST /mudu/repl/membership`（或 `mcli repl` 子命令）变更 raft 组成员，请求必须发给当前 leader（非 leader 返回 `leader_hint`，`mcli` 会自动重投到 hint 指向的节点）：

```bash
mcli --http-addr <任一节点>:8300 repl add-node --node-id 4 --host 10.0.0.4 --port 19527 --learner
mcli --http-addr <leader>:8300 repl promote-learner --node-id 4
mcli --http-addr <leader>:8300 repl remove-node --node-id 4
# 直接加为 voter（跳过 learner 阶段）：
mcli --http-addr <leader>:8300 repl add-node --node-id 4 --host 10.0.0.4 --port 19527
```

管理接口中的 `--node-id` 是成员名称经 FNV-1a 64-bit 派生的内部 ID，可从拓扑/状态接口取得；上面的 `4` 仅是命令参数示意，不能直接用于任意命名节点。

成员变更提交后，以持久共识配置为准。新增节点仍须使用当前名称式 `data_plane.group[].replica.member` 配置，并使启动配置、名称派生 ID、复制地址与期望拓扑一致；不要再使用旧 `peers` 的“自身加任一成员”配置片段。

- 变更走联合共识，一次一个；learner 不投票、不计入 quorum、不参与选举。
- 提升 learner 受追赶门控；切主期间失败应查明新 Leader 与当前成员状态后重试。
- 成员管理端点不自动重写本地配置，也不等价于更新全部组目录、placement 和控制面拓扑。
- 保留拓扑版本与在线成员协议的约束。已有管理代码和测试不代表任意拓扑变更或清空节点后的组级恢复均已验收。

## 可观测性

只读端点 `GET /mudu/server/replication` 返回本节点的复制状态视图（无能力位门控；单节点无复制时返回 `{"mode": "none"}` 而非 404）：

- 通用：`mode`（`"raft"` / `"stream"` / `"none"`）、`node_id`、`role`、`serving`（本节点是否接受写）。
- `raft` 段：`term`、`leader_node_id`、`commit_index`、`commit_vec`（per-worker 已提交 LSN + 段哈希）、`voters` / `learners`、`conf_version_committed` / `conf_version_new`（两者不等即有成员变更进行中）。
- `stream` 段：`epoch`（Primary 权威代际，不是事务 epoch）、`primary_node_id`、`is_primary`、`sync`、`sync_timeout_ms`。
- `workers`：每 worker `wal_next_lsn`（生产/应用高水位）、raft `chain_lsn` + `chain_seg_hash`、`staging_max_lsn`、最近快照点（`committed_lsn` / `taken_at_ms` / 目录字节）。
- `subscribers`：leader/primary 视角每订阅者 `(node_id, worker_index, stored_lsn, source_lsn, lag_lsn)`——`lag_lsn` 以 LSN（帧）计，即源端高水位与订阅者已落盘水位的差。
- `storage`：worker WAL / staging / raft_log / snapshot 目录字节数（**周期缓存**：后台线程每 10 秒刷新一次，端点只读缓存，不在请求路径扫描磁盘；首次刷新在启动时）。
- `counters`：`sync_write_timeouts`（sync 写超时次数）、`snapshots_taken`、`wal_chunks_reclaimed` / `staging_chunks_reclaimed`（前缀 GC 回收的 chunk 数）、`snapshot_offers_served` / `snapshot_restores_completed`。

CLI：`mcli --http-addr <节点>:8300 repl status`（JSON；`--table` 分节文本渲染）。示例（节选）：

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

`mcli server-topology` 的 `replication` 字段（role/leader/commit_vec）自本版本起随响应返回。

## 故障切换

**stream 形态（人工，mcli 工具化）**：

1. **隔离旧 Primary（fencing，运维责任）**。防止脑裂（新旧 Primary 同时接写）是运维的第一要务——确认旧 Primary 已下线或已隔离。
2. 提升新 Primary（在选定的 StreamFollower 上执行）：
   ```bash
   mcli --http-addr <StreamFollower>:8300 repl promote --fencing-cmd ./fence-old.sh
   # 或无钩子脚本时显式确认旧 Primary 已隔离：
   mcli --http-addr <StreamFollower>:8300 repl promote --yes
   ```
   `--fencing-cmd` 在本地执行、失败即中止；promote 缺 fencing 确认会被服务端以 400 拒绝。提升动作：Stream epoch 先落盘（`max(本地持久, 已知)+1`）→ 开写门 → 持久化 `stream_role`。
3. 其余 StreamFollower 改指新 Primary（无需重启，sink 自动重连）：
   ```bash
   mcli --http-addr <StreamFollower>:8300 repl follow --primary <新 Primary host:repl_port> --primary-node-id <N>
   ```
   follow 会先探活校验对端 node_id（防误指），之后 StreamFollower 的流自动重指并追平；重叠区按 CRC64 字节比对截断分歧。

**Stream epoch 数据面兜底（区别于事务 epoch）**：即使旧 Primary（僵尸）复活且仍自认 primary，它的 epoch 更低——StreamFollower 校验订阅首帧/批次的 epoch，低于已知即拒；指向它的 follow 探活同样被"stale primary"拒绝。事务 id 编码 `xid = (epoch << 32) | counter`，跨 Primary 单调（与 raft term 编码一致）。fencing 仍是运维责任：epoch 兜底的是数据面，脑裂期间旧 Primary 的本地写不会被新集群接收，但也无法自动撤销。

**stored wins**：`<db_path>/stream_role`（角色/epoch/Primary 地址）在重启时优先于 `mudud.cfg` 的 `replica.stream.bootstrap_primary` 键（与 raft stored conf 同哲学）——提升/改指过的节点重启后保持其新角色，不会因静态配置回退。不要通过删除该权威记录重置在线部署；提升和跟随操作须保持 fencing 与 epoch 单调性。

**raft 形态（自动）**：无需干预。选举超时内选出新 Leader 并自愈开写；旧 Leader 恢复后以 RaftFollower 身份重 join，按已提交流进度协调未提交尾并追平。

## 限制（当前版本）

- 仅支持 `server_mode = "Tokio"`；Legacy / IOUring 模式启用复制会拒绝启动。
- stream 形态无在线成员变更（raft 形态支持，见上节）；成员变更不改变各节点本地 `mudud.cfg`（建议手工同步）。
- 快照文件整文件读入内存生成/传输（大数据量节点的快照耗时与内存占用需注意）；快照恢复期间该 StreamFollower 并发读为尽力而为。
- relation_wal 暂不做前缀 GC（重放幂等，仅体积随时间增长）。
- stream 形态多个 StreamFollower（数据成员超过两个）配置允许，但写扇出未在验证范围内。
