# MuduDB 服务端配置契约 v1

## 范围

本文档规定 `mudud.cfg` 服务端配置文件格式。该文件控制服务端监听端口、执行模式、运行时路径与 io_uring 行为。

## 版本历史

| 版本 | 日期 | 摘要 |
|------|------|------|
| 1 | 2026-6-25 | 初始 TOML 配置。无显式 `version` 字段；兼容性通过 serde 默认值与别名隐式保证。 |
| 1 | 2026-10-12 | 补充记录已有的向后兼容字段（`wal_flush_max_wait_us`、`wal_sync_mode`、`wal_sync_interval_ms`、`[invocation_limits]`、`[replication]`、`[backup]`）；将文档中的 `server_mode` 默认值更正为 `"Tokio"`。无格式变更。 |
| 1 | 2026-11-20 | 集群拓扑重构为顶层 `control_plane`/`data_plane`；节点、数据组与集群改用唯一 `name` 标识，内部 id 由 FNV-1a 64 推导为 u64；新增显式 `version = 1`。破坏性变更，无兼容。 |

## 文件位置

服务端按以下顺序查找第一个存在的配置文件：

1. `--cfg /path/to/mudud.cfg`（或 `-c /path/to/mudud.cfg`）指定的路径（如果提供）。
2. 当前工作目录下的 `./mudud.cfg`。
3. 用户主目录下的 `~/.mududb/mudud.cfg`。

若上述文件都不存在，服务端返回 `NotFound` 错误。启动前请使用 `mudud init-cfg` 生成默认的 `./mudud.cfg`。

## 配置字段

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `mpk_path` | string | `"./mpk"` | 应用包目录路径。 |
| `db_path` | string | `"./data"` | 数据库文件目录路径。 |
| `listen_ip` | string | `127.0.0.1` | 监听 IP 地址。 |
| `http_listen_port` | u16 | `8300` | HTTP 管理 API 端口。 |
| `http_worker_threads` | usize | `1` | HTTP worker 线程数。 |
| `pg_listen_port` | u16 | `5432` | PostgreSQL wire protocol 端口。 |
| `component_target` | string | `p2` | Wasm 组件 ABI 目标。允许值：`p2`、`p3`。 |
| `enable_async` | boolean | `true` | 是否启用 WASI 组件运行时。 |
| `server_mode` | string | `"Tokio"` | `"Legacy"`、`"IOUring"` 或 `"Tokio"`。 |
| `tcp_listen_port` | u16 | `9527` | TCP 定帧协议端口。 |
| `tcp_multi_port` | boolean | `false` | 每个 worker 一个 TCP 监听器。 |
| `worker_threads` | usize | `0` | Worker 线程数。`0` 表示使用可用并行度。 |
| `io_uring_ring_entries` | u32 | `1024` | io_uring 完成队列深度。 |
| `io_uring_accept_multishot` | boolean | `true` | 启用 io_uring accept multishot。 |
| `io_uring_recv_multishot` | boolean | `true` | 启用 io_uring recv multishot。 |
| `io_uring_enable_fixed_buffers` | boolean | `false` | 启用 io_uring fixed buffers。 |
| `io_uring_enable_fixed_files` | boolean | `false` | 启用 io_uring fixed files。 |
| `routing_mode` | string | `"ConnectionId"` | `"ConnectionId"`、`"PlayerId"` 或 `"RemoteHash"`。 |
| `log_chunk_size` | u64 | `64 * 1024 * 1024` | io_uring 日志 chunk 大小，单位字节。 |
| `queue_max_msgs` | u64 | `65536` | 每队列最大保留消息数，必须大于零。 |
| `queue_max_bytes` | u64 | `67108864` | 每队列编码 payload 字节上限，必须大于零；不含系统列和存储开销。 |
| `page_size` | usize | `4096` | 数据库页大小，单位字节。该字段是持久化配置；已有数据库变更该值需要迁移或重新初始化。 |
| `wal_flush_max_wait_us` | u64 | `200` | 组提交批量窗口，单位微秒：WAL 提交批在队列中最多停留多久后由 flush 驱动刷盘。 |
| `wal_sync_mode` | string | `"commit"` | WAL 持久化模式：`"commit"` 每轮 flush 在应答提交前 fsync；`"periodic"` 在 WAL 写入进入页缓存后即应答，并至多每 `wal_sync_interval_ms` fsync 一次。 |
| `wal_sync_interval_ms` | u64 | `10` | `wal_sync_mode = "periodic"` 下的 WAL fsync 间隔，单位毫秒。掉电可能丢失最近一个间隔内已确认的提交。 |
| `version` | u32 | `1` | 配置格式版本；未知版本报错，不静默降级。 |
| `name` | string | （缺省） | 本节点唯一名字（集群模式）。 |
| `cluster` | string | （缺省） | 集群名字，全集群一致（集群模式）。 |
| `channel_listen_port` | u16 | （缺省） | 本地节点间消息监听端口（集群模式）。 |
| `topology_version` | u64 | （缺省） | 运行时拓扑修订/隔离值（集群模式）。 |
| `epoch_sparse` | boolean | `false` | 启用稀疏 epoch 协议（P4）：控制面在已提交的预留范围内于内存中推进 stable/current epoch，而不是每个 tick 提交一条 `Advance` 记录。协议推广期间默认关闭。 |

## 可选配置段

### `[invocation_limits]`

应用于每次过程调用的预算；变更对新构造的运行时生效。缺失的键使用下列默认值。未知键会被拒绝（`deny_unknown_fields`）。

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `memory_bytes` | usize | `128 * 1024 * 1024` | 单次调用中所有 memory 的线性内存总字节数。 |
| `fuel` | u64 | `1000000000` | Wasm 指令 fuel，含组件初始化。 |
| `timeout_ms` | u64 | `30000` | 墙钟执行预算，含事务建立。 |
| `max_input_bytes` | usize | `8 * 1024 * 1024` | 编码后过程或宿主调用的最大输入大小。 |
| `max_output_bytes` | usize | `8 * 1024 * 1024` | 编码后过程或宿主调用的最大输出大小。 |
| `max_host_calls` | u64 | `10000` | 最大宿主调用次数，含打开资源的调用。 |
| `max_resources` | usize | `256` | 同时打开的 worker 会话与文件句柄的最大数量。 |
| `table_elements` | usize | `100000` | 单次调用中表元素的最大总数。 |
| `max_concurrent` | usize | `4` | 进程级最大准入过程调用数。 |
| `max_concurrent_per_app` | usize | `2` | 同一应用名下最大准入调用数。 |
| `total_memory_bytes` | usize | `512 * 1024 * 1024` | 进程级预留的 guest 线性内存预算。 |

### 集群拓扑：`[control_plane]` 与 `[data_plane]`

> 当前启动边界：数据组 Raft 参数已生效，但独立控制组的 Raft 参数尚未接入 `ControlPlaneNode::start`，该路径仍构造 `RaftCfg::new(..., 25, 4, ...)`。数据节点实际 Worker 数取顶层 `worker_threads`（0 为机器并行度），须显式与组 `worker_count` 一致。一体式（Standalone）只有在已有数据 Raft 时共享控制/数据共识；stream 或无数据复制时仍另建控制 Raft。集群配置当前使用 Tokio，IOUring 拒绝 `data_plane`。见[装配边界](../cluster_configuration.zh-CN.md)。

集群模式由顶层 `name`、`cluster`、`channel_listen_port`、`topology_version` 及 `[control_plane]`/`[data_plane]` 段组成。这些字段缺省时服务端以单节点模式运行，不启动任何集群组件。

节点、数据组与集群用唯一 `name`（字符串）标识，配置里不出现数字 id；内部数字 id 由 FNV-1a 64 位哈希确定性推导（offset basis `0xcbf29ce484222325`、prime `0x100000001b3`，无 salt/seed），全部为 u64：

| 内部 id | 来源 |
|---|---|
| `node_id` | `fnv1a64(name)`（本节点或成员名字的 UTF-8 字节） |
| `group_id` | `fnv1a64(group.name)`（组名字的 UTF-8 字节） |
| `cluster_id` | `fnv1a64(cluster)`（集群名字的 UTF-8 字节） |

`[control_plane]`（可选；省略即一体式 Standalone，要求恰好一个数据组）：

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `replica.raft` | table | （必填） | 独立控制组的复制模式（仅 Raft）。 |
| `replica.member` | table 数组 | （必填） | `[[control_plane.replica.member]]` 控制启动成员。 |

`[data_plane]`：

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `group` | table 数组 | （必填） | `[[data_plane.group]]` 数据组列表。 |
| `placements` | table 数组 | `[]` | `[[data_plane.placements]]` 静态放置记录。 |
| `rebalance_interval_ms` | u64 | `0` | 重平衡周期；0 关闭调度。 |

`[[data_plane.group]]` 字段：

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `name` | string | （必填） | 数据组唯一名字。 |
| `worker_count` | u32 | （必填） | 该组每个成员的业务 Worker 数。 |
| `replica.raft` | table | （至多选一） | 选择 Raft 模式；与 `replica.stream` 互斥。 |
| `replica.stream` | table | （至多选一） | 选择流复制模式；与 `replica.raft` 互斥。 |
| `replica.member` | table 数组 | （必填） | `[[data_plane.group.replica.member]]` 该组启动成员。 |

`replica.raft` 与 `replica.stream` 都缺省时，该数据组为单副本（无复制机制），`replica.member` 必须恰好一个成员；两者都声明则报错。

`[data_plane.group.replica.raft]` 字段：

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `tick_ms` | u64 | `50` | Raft 状态机 tick 间隔，单位毫秒。 |
| `election_timeout_ticks` | u64 | `500` | Raft 选举超时，单位 tick。 |

`[data_plane.group.replica.stream]` 字段：

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `bootstrap_primary` | string | （必填） | 初始 primary 的成员名字；仅用于首次启动。 |
| `sync` | boolean | （必填） | primary 是否等待至少一个 StreamFollower 的持久化确认后才应答提交。 |
| `sync_timeout_ms` | u64 | `10000` | 同步复制等待超时，单位毫秒。 |

`[[control_plane.replica.member]]` 字段：

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `name` | string | （必填） | 控制成员唯一名字。 |
| `host` | string | （必填） | 控制成员通道监听的主机名或 IP。 |
| `channel_port` | u16 | （必填） | 控制成员通道端口。 |
| `client_port` | u16 | （必填） | 控制成员客户端端口。 |

`[[data_plane.group.replica.member]]` 字段：

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `name` | string | （必填） | 数据成员唯一名字。 |
| `host` | string | （必填） | 数据成员通道监听的主机名或 IP。 |
| `channel_port` | u16 | （必填） | 数据成员通道端口。 |
| `client_port` | u16 | （必填） | 数据成员客户端端口。 |
| `replication_port` | u16 | （必填） | 数据成员复制通道端口。 |

`[[data_plane.placements]]` 字段：

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `rule` | string | （必填） | 分区规则名字。 |
| `partition` | string | （必填） | 分区名字。 |
| `group` | string | （必填） | 引用的数据组名字。 |
| `worker_index` | u32 | （必填） | 组内 Worker 槽位下标。 |

### `[backup]`

备份与时间点恢复。该段缺省时服务端以无备份模式运行：WAL 前缀 GC 直接回收 chunk 而不归档。该段存在时，密封的 WAL chunk 在 GC 回收前会持续归档到 `archive_dir`。

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `archive_dir` | string | （缺省） | WAL chunk 归档目录（持续归档）。 |
| `backup_dir` | string | （缺省） | 基础备份写入目录（`mcli backup`）。 |
| `archive_mode` | string | `"on"` | 归档模式：`"off"`、`"on"` 或 `"always"`。 |

## 兼容性说明

- 配置文件携带显式 `version = 1`；未知版本报错，不静默降级。
- 节点、数据组与集群用唯一名字标识；内部 id 由 FNV-1a 64 哈希推导为 u64。哈希函数固化进契约，更换哈希函数属于破坏性变更。
- `serde(default)` 保证缺失字段使用默认值。

## 升级与回滚规则

- **升级：** 格式版本当前为 1。将来引入破坏性变更时提升 `version`，并提供离线配置迁移工具。
- **回滚：** `version` 不匹配时解析失败并返回解码错误，不会修改文件。
- **迁移：** 增量化变更不需要迁移工具；破坏性变更需要提供离线配置迁移工具。

## 废弃策略

- 引入破坏性变更时提升 `version` 字段并添加迁移工具。

## 参考

- 解析器：[mudu_runtime/src/backend/mudud_cfg/mod.rs](../../../crates/db-kernel/mudu_runtime/src/backend/mudud_cfg/mod.rs)
- 示例配置：[`mudud init-cfg`](../running_mudud.zh-CN.md)
