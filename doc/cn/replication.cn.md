# 复制与 Raft 容错

MuduDB 支持三种部署形态：**单机**（默认）、**主从流复制**（stream）、**Raft 共识容错**（raft）。在 `mudud.cfg` 中加入 `[replication]` 段即启用集群；缺省不加时单机运行，行为与无复制能力完全一致。

两种集群形态共用同一条复制通道：复制流与 Raft 消息按消息类型复用 `listen_port`，无需独立端口。

## 部署形态

| 形态 | 配置 | 写应答条件 | 主故障切换 | 已应答提交的 RPO |
|------|------|-----------|-----------|-----------------|
| 单机 | 无 `[replication]` | 本地 WAL 落盘（按 `wal_sync_mode`） | — | — |
| 主从（异步） | `[replication.stream]` `sync = false` | 本地 WAL 落盘即应答 | 人工 | > 0 |
| 主从（同步） | `[replication.stream]` `sync = true` | 任一从库落盘确认后应答 | 人工 | 0（前提：故障时至少一台从库存活） |
| Raft HA | `[replication.raft]`，3 或 5 节点 | 多数派落盘 + 共识提交后应答 | 自动（选举 + 自愈） | 0 |

选择建议：只需要读写分离/数据冗余并能接受人工切换，用 stream；需要无人值守的故障自动转移，用 raft（至少 3 节点）。

## 工作原理

**数据面（两种形态共用）**：每个 worker 一条复制流，主库把 WAL 段（64 KiB / 32 帧 / 200 µs 成批）有序推送给从库，带滑动窗口流控（`repl_flow_window_segments`）；从库只读，写请求被拒绝并返回 `leader_hint`，客户端据此自动重连到主库。

**stream 形态**：主库本地 WAL 提交即 apply。`sync = true` 时，应答前等待任一从库确认已落盘到该提交点，`sync_timeout_ms` 超时则向客户端返回错误（数据已本地落盘，属"未确认复制"，从库追平后自愈）。

**raft 形态**：全节点单 Raft 组（`mudu_raft`，scupt-raft 的仓内 fork，TLA+ 验证算法），日志条目只含段哈希与提交向量 `CommitVec{[(worker_id, lsn, seg_hash); W]}`，一次共识推进全部 worker 流。两条不变量保证 RPO=0：

- 多数派落盘门控：多数派从库确认持有某段后，才提交覆盖该段的 CommitVec；
- apply ≤ 已提交向量点：从库先把段写入 staging，只重放 raft 已提交的部分；未提交尾永不入存储，切换时截断零成本。

主挂掉后，剩余节点在选举超时内选出新主；新主**先自愈再服务**——按最后提交的 CommitVec 逐 worker 比对哈希链定位分歧、拉段追平、写入 Barrier 后开写。事务 id 编码 `xid = (raft_term << 32) | counter`，跨主单调。

## 配置

`[replication]` 段放共有字段；模式由子段二选一决定——`[replication.stream]` 或 `[replication.raft]` 存在即选定模式，两者皆缺或并存都是配置错误。

### 主从流复制（双节点示例）

节点 1（主）的 `mudud.cfg`：

```toml
mpk_path = "./mpk"
db_path = "./data"
listen_ip = "0.0.0.0"
http_listen_port = 8300
pg_listen_port = 5432
enable_async = true
server_mode = "Tokio"

[replication]
node_id = 1                 # 首启持久化到 <db_path>/node_id，之后不一致拒绝启动
listen_port = 19527         # 复制 + Raft 共用端口，不得与 http/pg/tcp 端口冲突
repl_flow_window_segments = 64   # 可选，默认 64
snapshot_wal_threshold_bytes = 1073741824   # 可选，默认 1 GiB：committed WAL 越过上次快照该量即触发下一快照
gc_check_interval_secs = 60                # 可选，默认 60：WAL/staging GC 评估间隔

[[replication.peers]]       # 静态成员表，各节点内容一致，必须包含自身 node_id
node_id = 1
host = "10.0.0.1"
port = 19527

[[replication.peers]]
node_id = 2
host = "10.0.0.2"
port = 19527

[replication.stream]
primary = true              # 必填：恰好一台为 true，其余为 false
sync = true                 # 必填：true=同步，false=异步
sync_timeout_ms = 10000     # 可选，默认 10000
```

节点 2（从）与节点 1 仅两处不同：`node_id = 2`，`primary = false`。

### Raft 容错（三节点示例）

共有部分同上，每个节点的 `node_id` 分别为 1/2/3，模式子段换成：

```toml
[replication.raft]
raft_tick_ms = 50              # 可选，默认 50；Raft 状态机 tick 间隔
election_timeout_ticks = 500   # 可选，默认 500；选举超时 = tick × 该值（默认 25s）
```

`peers` 列全部三台。节点数必须 ≥ 3（或 1 用于开发调试）：**双节点 raft 组作为稳态部署不推荐**（无自动容错——多数派 = 2 挂一台即停摆，写可用性反而低于单机），启动时打警告；仅在新节点以"自身 + 一个现存成员"的最小配置 bootstrap 入网时是合法形态（stored conf wins，入网后由集群 conf 取代）。

### 校验规则

- 两个模式子段恰好出现一个；`primary`/`sync` 在 `[replication.stream]` 内为必填键。
- `peers` 非空、node_id 与地址均唯一、必须包含自身 `node_id`。
- `listen_port` 不得与 `http_listen_port` / `pg_listen_port` / `tcp_listen_port` 冲突。
- `node_id` 首启持久化到 `<db_path>/node_id`；改配置与持久值不一致时拒绝启动。
- `snapshot_wal_threshold_bytes` / `gc_check_interval_secs` 必须大于 0。

## 快照与 WAL GC

每个 worker 在 committed LSN 越过 `上次快照 + snapshot_wal_threshold_bytes` 时独立做业务快照（无需 raft entry，各节点在同一点各自进行）：先 flush 脏页并 fsync 数据文件（这是新增的持久化动作，阈值越大开销越稀），再把该分区的 `relation/*.dat` 与内存 kv 存储的 dump（保留原始 xid）写入 `<db_path>/snapshot/worker_N/v<lsn>/`，最后落盘 manifest（含 committed_lsn、段哈希、文件 CRC64 清单）即发布。

快照让两类回收成为可能：

- **WAL 前缀 GC**：低于水位的整 chunk 物理删除，水位 = 主库 min(在线订阅者确认水位, 已提交点, 最近快照点)、从库 min(已提交点, 最近快照点)；`<db_path>/*.gcfrontier` 标记文件保证重启后从 frontier 继续，不会把日志误判为空。staging 与 raft_log 同节奏回收（业务快照点同时作为 raft 快照安装前沿）。
- **重 join 快速路径**：从库重连时的订阅点若低于主库 GC frontier，主库改答"快照 offer"——从库分页拉取快照文件、校验 CRC、替换数据文件并恢复 kv 存储、把 raft 前沿安装到快照点后，从 `snapshot_lsn+1` 重新订阅，避免 WAL 全量重放。已断开连接的从库不再拖住主库的 WAL 回收。

注意：快照点之后的第一批写仍会推流补齐；快照传输期间源端继续写不受影响（快照以 manifest 点自封）。relation_wal 的前缀 GC 本版未做（其页 LSN 戳保证重放幂等，体积增长独立于 worker WAL）。

## 在线成员变更（raft 形态）

运行中可通过管理 HTTP 端点 `POST /mudu/repl/membership`（或 `mcli repl` 子命令）变更 raft 组成员，请求必须发给当前 leader（非 leader 返回 `leader_hint`，`mcli` 会自动重投到 hint 指向的节点）：

```bash
mcli --http-addr <任一节点>:8300 repl add-node --node-id 4 --host 10.0.0.4 --port 19527 --learner
mcli --http-addr <leader>:8300 repl promote-learner --node-id 4
mcli --http-addr <leader>:8300 repl remove-node --node-id 4
# 直接加为 voter（跳过 learner 阶段）：
mcli --http-addr <leader>:8300 repl add-node --node-id 4 --host 10.0.0.4 --port 19527
```

推荐的新节点入网流程（learner 两步法）：

1. 在 leader 上 `add-node --learner` 登记新成员（含其复制地址）；joint 共识提交即生效。
2. 启动新节点：本地 `mudud.cfg` 的 `peers` 只需配**自身 + 任一现存成员**（两 peers 的 raft 配置仅警告，是合法的 bootstrap 形态）——集群已提交的 conf 会覆盖本地静态配置（stored conf wins）；新节点经快照/复制流追平。
3. 追平后 `promote-learner` 使其成为 voter 参与选举与多数派。

注意事项：

- 变更通过 raft 联合共识提交，一次只能进行一个变更；reconf 期间 leader 易主时请求会明确失败（可在新 leader 上重发，新主会从自身状态继续驱动）。
- learner 复制日志但不投票、不参与多数派计数，也永不被选举。
- 地址簿由内核侧维护（conf 只携带 node_id）：`add-node` 时登记 host/port，remove 后自动清理。
- 在线变更后建议手工同步各节点 `mudud.cfg` 的 `peers`（仅影响首启 bootstrap；运行中以集群已提交 conf 为准）。
- 选举超时是纯本地配置（不随 conf 复制），各节点相位在成员变更后依然确定。

## 可观测性

只读端点 `GET /mudu/server/replication` 返回本节点的复制状态视图（无能力位门控；单机无复制时返回 `{"mode": "none"}` 而非 404）：

- 通用：`mode`（`"raft"` / `"stream"` / `"none"`）、`node_id`、`role`、`serving`（本节点是否接受写）。
- `raft` 段：`term`、`leader_node_id`、`commit_index`、`commit_vec`（per-worker 已提交 LSN + 段哈希）、`voters` / `learners`、`conf_version_committed` / `conf_version_new`（两者不等即有成员变更进行中）。
- `stream` 段：`epoch`、`primary_node_id`、`is_primary`、`sync`、`sync_timeout_ms`。
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

1. **隔离旧主（fencing，运维责任）**。防止脑裂（新旧主同时接写）是运维的第一要务——确认旧主已下线或已隔离。
2. 提升新主（在选定的从库上执行）：
   ```bash
   mcli --http-addr <从库>:8300 repl promote --fencing-cmd ./fence-old.sh
   # 或无钩子脚本时显式确认旧主已隔离：
   mcli --http-addr <从库>:8300 repl promote --yes
   ```
   `--fencing-cmd` 在本地执行、失败即中止；promote 缺 fencing 确认会被服务端以 400 拒绝。提升动作：epoch 先落盘（`max(本地持久, 已知)+1`）→ 开写门 → 持久化 `stream_role`。
3. 其余从库改指新主（无需重启，sink 自动重连）：
   ```bash
   mcli --http-addr <从库>:8300 repl follow --primary <新主host:repl_port> --primary-node-id <N>
   ```
   follow 会先探活校验对端 node_id（防误指），之后从库的流自动重指并追平；重叠区按 CRC64 字节比对截断分歧。

**epoch 数据面兜底**：即使旧主（僵尸）复活且仍自认 primary，它的 epoch 更低——从库校验订阅首帧/批次的 epoch，低于已知即拒；指向它的 follow 探活同样被"stale primary"拒绝。事务 id 编码 `xid = (epoch << 32) | counter`，跨主单调（与 raft term 编码一致）。fencing 仍是运维责任：epoch 兜底的是数据面，脑裂期间旧主的本地写不会被新集群接收，但也无法自动撤销。

**stored wins**：`<db_path>/stream_role`（角色/epoch/主库地址）在重启时优先于 `mudud.cfg` 的 `primary` 键（与 raft stored conf 同哲学）——提升/改指过的节点重启后保持其新角色，不会因静态配置回退。如需强制回到配置态，删除 `stream_role` 并重启。

**raft 形态（自动）**：无需干预。选举超时内选出新主并自愈开写；旧主恢复后自动以从库身份重 join，按 CommitVec 截断未提交分叉尾并追平。

## 限制（当前版本）

- 仅支持 `server_mode = "Tokio"`；Legacy / IOUring 模式启用复制会拒绝启动。
- stream 形态无在线成员变更（raft 形态支持，见上节）；成员变更不改变各节点本地 `mudud.cfg`（建议手工同步）。
- 快照文件整文件读入内存生成/传输（大数据量节点的快照耗时与内存占用需注意）；快照恢复期间该从库并发读为尽力而为。
- relation_wal 暂不做前缀 GC（重放幂等，仅体积随时间增长）。
- stream 形态多从库（peers > 2）配置允许，但写扇出未在验证范围内。

实现设计与待办详见 [Replication + Raft 设计 Todo](../dev/todo/replication_raft_todo.md)。
