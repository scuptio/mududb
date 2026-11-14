# MuduDB 服务端配置契约 v1

## 范围

本文档规定 `mudud.cfg` 服务端配置文件格式。该文件控制服务端监听端口、执行模式、运行时路径与 io_uring 行为。

## 版本历史

| 版本 | 日期 | 摘要 |
|------|------|------|
| 1 | 2026-6-25 | 初始 TOML 配置。无显式 `version` 字段；兼容性通过 serde 默认值与别名隐式保证。 |
| 1 | 2026-10-12 | 补充记录已有的向后兼容字段（`wal_flush_max_wait_us`、`wal_sync_mode`、`wal_sync_interval_ms`、`[invocation_limits]`、`[replication]`、`[backup]`）；将文档中的 `server_mode` 默认值更正为 `"Tokio"`。无格式变更。 |

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

### `[replication]`

主从流复制与 Raft 容错配置。该段缺省时服务端以单机模式运行，不启动任何复制组件。`[replication.stream]` 与 `[replication.raft]` 两个子段必须恰好出现一个，出现的子段即选定模式。

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `node_id` | u64 | （必填） | 本节点标识。首次启动时持久化到 `<db_path>/node_id`；与持久化值不一致属于启动错误。 |
| `listen_port` | u16 | （必填） | 复制与 Raft 通道监听端口。 |
| `repl_flow_window_segments` | u64 | `64` | 复制流的流控窗口，单位为段。 |
| `snapshot_wal_threshold_bytes` | u64 | `1073741824` | 距上次快照后已提交 WAL 字节数超过该值时触发下一次 per-worker 快照。 |
| `gc_check_interval_secs` | u64 | `60` | WAL/staging GC 水位评估间隔，单位秒。 |
| `stream` | table | （缺省） | `[replication.stream]` 子段；选择流复制模式。 |
| `raft` | table | （缺省） | `[replication.raft]` 子段；选择 Raft 共识模式。 |
| `peers` | table 数组 | `[]` | `[[replication.peers]]` 静态集群成员；必须包含本节点的 `node_id`。 |

`[replication.stream]` 字段：

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `primary` | boolean | （必填） | 本节点是否为指定 primary；集群中恰好一个成员设为 `true`。 |
| `sync` | boolean | （必填） | primary 是否等待某个 replica 的持久化确认后才应答提交。 |
| `sync_timeout_ms` | u64 | `10000` | 同步复制等待超时，单位毫秒。 |

`[replication.raft]` 字段：

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `raft_tick_ms` | u64 | `50` | Raft 状态机 tick 间隔，单位毫秒。 |
| `election_timeout_ticks` | u64 | `500` | Raft 选举超时，单位 tick。 |

`[[replication.peers]]` 字段：

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `node_id` | u64 | （必填） | 对等节点标识。 |
| `host` | string | （必填） | 对等节点复制通道监听的主机名或 IP 地址。 |
| `port` | u16 | （必填） | 对等节点复制通道监听端口。 |

### `[backup]`

备份与时间点恢复。该段缺省时服务端以无备份模式运行：WAL 前缀 GC 直接回收 chunk 而不归档。该段存在时，密封的 WAL chunk 在 GC 回收前会持续归档到 `archive_dir`。

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `archive_dir` | string | （缺省） | WAL chunk 归档目录（持续归档）。 |
| `backup_dir` | string | （缺省） | 基础备份写入目录（`mcli backup`）。 |
| `archive_mode` | string | `"on"` | 归档模式：`"off"`、`"on"` 或 `"always"`。 |

## 兼容性说明

- 当前文件中无显式 `version` 字段。格式版本是隐式的，由解析器识别的字段集合决定。
- `serde(default)` 保证缺失字段使用默认值，使使用当前字段名的旧配置文件可在新二进制上加载。

## 升级与回滚规则

- **升级：** 引入 v2 配置格式时，将要求显式 `version = 2` 字段。若新增可选字段且使用 `serde(default)`，可不提升 v1 版本号。
- **回滚：** 新二进制可读取使用当前字段名的旧配置文件（得益于默认值）。v1-only 二进制遇到 v2 配置时因未知字段解析失败并返回解码错误，不会修改文件。
- **迁移：** 增量化变更不需要迁移工具。破坏性变更需要提供离线配置迁移工具。

## 废弃策略

- 引入破坏性变更时将添加显式 `version` 字段。

## 参考

- 解析器：[mudu_runtime/src/backend/mudud_cfg/mod.rs](../../../crates/db-kernel/mudu_runtime/src/backend/mudud_cfg/mod.rs)
- 示例配置：[`mudud init-cfg`](../running_mudud.cn.md)
