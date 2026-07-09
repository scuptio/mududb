# 运行 `mudud`

本文档介绍如何配置并运行 MuduDB 服务器 `mudud`。

## `mudud` 是什么？

`mudud` 是 MuduDB 的服务端进程。它承载数据库内核、运行已安装的MPK包、向客户端暴露 TCP 协议端口、HTTP 管理端口，以及兼容 PostgreSQL 的 wire protocol 端口。

## 前置条件

- `mudud` 和 `mcli` 已安装并在 `PATH` 中。
- 已选择工作目录，`mudud` 将在该目录读取 `mudud.cfg`，并存放数据。
- 在 Linux 上使用 `IOUring` 模式需要运行时存在 `liburing-dev`。

## 配置文件

`mudud` 按以下顺序查找配置文件：

1. `--cfg` / `-c` 指定的路径（如果提供）。该路径会被直接使用。
2. 当前工作目录下的 `./mudud.cfg`。
3. 用户主目录下的 `~/.mududb/mudud.cfg`。

第一个存在的文件会被加载。如果都不存在，`mudud` 返回 `NotFound` 错误并退出，不会自动创建文件。

### 生成默认配置

在不启动服务器的情况下写入默认的 `./mudud.cfg`：

```bash
mudud init-cfg
```

### 使用自定义配置文件

```bash
mudud serve --cfg /path/to/mudud.cfg
```

### `mudud.cfg` 示例

```toml
# 存放 .mpk 应用包的目录。
mpk_path = "./mpk"

# 数据库存储文件目录。
db_path = "./data"

# 监听 IP 地址。
listen_ip = "127.0.0.1"

# HTTP 管理 API 端口。
http_listen_port = 8300

# HTTP 工作线程数。
http_worker_threads = 1

# PostgreSQL wire protocol 端口。
pg_listen_port = 5432

# MuduDB 协议 TCP 端口，供 mcli 使用。
tcp_listen_port = 9527

# 服务器执行模式："Legacy"、"IOUring"（Linux 推荐）或 "Tokio"。
server_mode = "Tokio"

# 工作线程数。0 表示自动检测 CPU 核心数。
worker_threads = 0

# io_uring completion queue ring entries。
io_uring_ring_entries = 1024

# 启用 io_uring accept/receive multishot 优化。
io_uring_accept_multishot = true
io_uring_recv_multishot = true

# 启用 io_uring fixed buffers/files（实验性）。
io_uring_enable_fixed_buffers = false
io_uring_enable_fixed_files = false

# TCP 路由模式："ConnectionId"、"PlayerId" 或 "RemoteHash"。
routing_mode = "ConnectionId"

# 异步运行时支持。
enable_async = true

# 为 worker 使用多个连续 TCP 端口。
tcp_multi_port = false

# log chunk 大小，单位为字节。
log_chunk_size = 67108864

# 数据库页大小，单位为字节。持久化设置：修改后需要重新初始化。
page_size = 4096
```

### 配置项说明

| 配置项 | 默认值 | 说明 |
|--------|--------|------|
| `mpk_path` | 临时目录 | 存放 `.mpk` 应用包的目录。 |
| `db_path` | 临时目录 | 数据库存储文件目录。 |
| `listen_ip` | `127.0.0.1` | 服务器监听 IP 地址。 |
| `http_listen_port` | `8300` | HTTP 管理 API 端口。 |
| `http_worker_threads` | `1` | HTTP 工作线程数。 |
| `pg_listen_port` | `5432` | PostgreSQL wire protocol 端口。 |
| `tcp_listen_port` | `9527` | 供 `mcli` 使用的 MuduDB TCP 协议端口。 |
| `server_mode` | `Tokio` | 后端执行模式：`Legacy`、`IOUring` 或 `Tokio`。Linux 用户建议使用 `IOUring`。 |
| `worker_threads` | `0` | 工作线程数。`0` 表示自动检测 CPU 核心数。 |
| `io_uring_ring_entries` | `1024` | io_uring completion queue ring entries。 |
| `io_uring_accept_multishot` | `true` | 启用 io_uring accept multishot。 |
| `io_uring_recv_multishot` | `true` | 启用 io_uring receive multishot。 |
| `io_uring_enable_fixed_buffers` | `false` | 启用 io_uring fixed buffers（实验性）。 |
| `io_uring_enable_fixed_files` | `false` | 启用 io_uring fixed files（实验性）。 |
| `routing_mode` | `ConnectionId` | TCP 连接路由策略：`ConnectionId`、`PlayerId` 或 `RemoteHash`。 |
| `enable_async` | `true` | 为 WASM 过程启用异步运行时支持。 |
| `tcp_multi_port` | `false` | 为 worker 使用多个连续 TCP 端口。 |
| `log_chunk_size` | `67108864` | io_uring log chunk 大小，单位为字节。 |
| `page_size` | `4096` | 数据库页大小。持久化设置：对已有数据库修改后需要重新初始化。 |

## 启动服务器

### 默认启动

```bash
ulimit -n 65535
mudud
```

如果当前目录不存在 `./mudud.cfg`，`mudud` 会继续查找 `~/.mududb/mudud.cfg`。如果这两个文件都不存在，请在启动前使用 `mudud init-cfg` 创建默认配置文件。

### 使用自定义配置启动

```bash
ulimit -n 65535
mudud serve --cfg ./config/mudud.cfg
```

### 启动后打开的端口

启动后，`mudud` 会记录有效配置并打开三个监听：

- TCP 协议：`listen_ip:tcp_listen_port`（默认 `127.0.0.1:9527`）
- HTTP 管理：`listen_ip:http_listen_port`（默认 `127.0.0.1:8300`）
- PostgreSQL wire protocol：`listen_ip:pg_listen_port`（默认 `127.0.0.1:5432`）

## 停止服务器

向进程发送 `SIGINT`（`Ctrl+C`）或 `SIGTERM`。`mudud` 会执行优雅关闭，等待在途请求处理完成后再退出。

## 验证服务器

使用 `mcli` 检查 HTTP 管理端口：

```bash
mcli --http-addr 127.0.0.1:8300 app-list
mcli --http-addr 127.0.0.1:8300 server-topology
```

如果命令返回 JSON 输出，说明服务器已启动并可访问。

## 常见问题

- **端口被占用**：其他进程占用了配置中的某个端口。修改 `mudud.cfg` 中冲突的端口。
- **权限不足**：可能没有权限绑定到配置的 `listen_ip` 或端口。本地开发建议使用 `127.0.0.1` 和 1024 以上的端口。
- **打开文件数过多**：启动前使用 `ulimit -n 65535` 提高文件描述符限制。
- **io_uring 不可用**：如果不在 Linux 上或缺少 `liburing-dev`，将 `server_mode` 切换为 `Tokio`。

## 下一步

- [mcli 管理接口](mcli_admin.cn.md) — 安装、列出和调用应用包。
- [你的第一个 MPK 包](your_first_mpk.cn.md) — 构建并安装完整示例。
- [核心概念](concepts.cn.md) — 了解 Mudu Procedure、MPK 包和运行时模型。
