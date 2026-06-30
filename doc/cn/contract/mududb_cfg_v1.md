# MuduDB 服务端配置契约 v1

## 范围

本文档规定 `mududb_cfg.toml` 服务端配置文件格式。该文件控制服务端监听端口、执行模式、运行时路径与 io_uring 行为。

## 版本历史

| 版本 | 日期 | 摘要 |
|------|------|------|
| 1 | 2026-6-25 | 初始 TOML 配置。无显式 `version` 字段；兼容性通过 serde 默认值与别名隐式保证。 |

## 文件位置

服务端按以下顺序加载配置：

1. `--cfg /path/to/mududb_cfg.toml` 指定的路径；
2. 默认位置 `${HOME}/.mududb/mududb_cfg.toml`。

若文件不存在，首次启动时服务端会使用默认值创建它。

## 配置字段

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `mpk_path` | string | 临时目录 | 应用包目录路径。 |
| `db_path` | string | 临时目录 | 数据库文件目录路径。别名：`data_path`。 |
| `listen_ip` | string | `127.0.0.1` | 监听 IP 地址。 |
| `http_listen_port` | u16 | `8300` | HTTP 管理 API 端口。 |
| `http_worker_threads` | usize | `1` | HTTP worker 线程数。 |
| `pg_listen_port` | u16 | `5432` | PostgreSQL wire protocol 端口。 |
| `component_target` | string | `p2` | Wasm 组件 ABI 目标。允许值：`p2`、`p3`。 |
| `enable_async` | boolean | `true` | 是否启用 WASI 组件运行时。 |
| `server_mode` | integer | `0` | `0` = Legacy，`1` = IOUring，`2` = Tokio。 |
| `tcp_listen_port` | u16 | `9527` | TCP 定帧协议端口。 |
| `tcp_multi_port` | boolean | `false` | 每个 worker 一个 TCP 监听器。 |
| `worker_threads` | usize | `0` | Worker 线程数。`0` 表示使用可用并行度。别名：`io_uring_worker_threads`。 |
| `io_uring_ring_entries` | u32 | `1024` | io_uring 完成队列深度。 |
| `io_uring_accept_multishot` | boolean | `true` | 启用 io_uring accept multishot。 |
| `io_uring_recv_multishot` | boolean | `true` | 启用 io_uring recv multishot。 |
| `io_uring_enable_fixed_buffers` | boolean | `false` | 启用 io_uring fixed buffers。 |
| `io_uring_enable_fixed_files` | boolean | `false` | 启用 io_uring fixed files。 |
| `routing_mode` | integer | `0` | `0` = ConnectionId，`1` = PlayerId，`2` = RemoteHash。 |
| `io_uring_log_chunk_size` | u64 | `64 * 1024 * 1024` | io_uring 日志 chunk 大小，单位字节。 |
| `page_size` | usize | `4096` | 数据库页大小，单位字节。该字段是持久化配置；已有数据库变更该值需要迁移或重新初始化。 |

## 兼容性说明

- 当前文件中无显式 `version` 字段。格式版本是隐式的，由解析器识别的字段集合决定。
- `serde(default)` 保证缺失字段使用默认值，使旧配置文件可在新二进制上加载。
- 字段别名（`data_path` → `db_path`，`io_uring_worker_threads` → `worker_threads`）保证与旧配置文件兼容。

## 升级与回滚规则

- **升级：** 引入 v2 配置格式时，将要求显式 `version = 2` 字段。若新增可选字段且使用 `serde(default)`，可不提升 v1 版本号。
- **回滚：** 新二进制可读取旧配置文件（得益于默认值与别名）。v1-only 二进制遇到 v2 配置时因未知字段解析失败并返回解码错误，不会修改文件。
- **迁移：** 增量化变更不需要迁移工具。破坏性变更需要提供离线配置迁移工具。

## 废弃策略

- 别名仅在经历两个完整发布周期且所有文档与示例配置更新后才可移除。
- 引入破坏性变更时将添加显式 `version` 字段。

## 参考

- 解析器：[mudu_runtime/src/backend/mududb_cfg.rs](../../../mudu_runtime/src/backend/mududb_cfg.rs)
- 示例配置：[doc/cfg/mududb_cfg.toml](../../cfg/mududb_cfg.toml)
