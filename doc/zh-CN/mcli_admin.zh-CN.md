# mcli 管理接口（HTTP）

本文介绍如何通过 `mcli` 的管理命令完成应用管理、路由查询、复制管理以及备份/恢复。

## 适用范围

- 适用于 `mcli` 通过 `--http-addr` 连接 `mudud` HTTP 管理接口的场景。
- 管理命令走 HTTP API，不需要手写 HTTP 请求。
- 如果服务端未启用对应管理能力，会返回错误信息（例如 `admin service is not configured`）。

## 前置条件

1. 启动 `mudud`。
2. 确认 HTTP 管理端口可访问（默认 `127.0.0.1:8300`）。

## 命令一览

### 1) 列出已安装应用

```bash
mcli --http-addr 127.0.0.1:8300 app-list
```

### 2) 安装应用包（`.mpk`）

```bash
mcli --http-addr 127.0.0.1:8300 app-install --mpk target/wasm32-wasip2/release/wallet.mpk
```

### 3) 查看应用过程列表

```bash
mcli --http-addr 127.0.0.1:8300 app-detail --app wallet
```

### 4) 查看单个过程详情

```bash
mcli --http-addr 127.0.0.1:8300 app-detail --app wallet --module wallet --proc create_user
```

### 5) 卸载应用

```bash
mcli --http-addr 127.0.0.1:8300 app-uninstall --app wallet
```

### 6) 查看服务拓扑

```bash
mcli --http-addr 127.0.0.1:8300 server-topology
```

### 7) 分区路由查询

按精确 key 路由：

```bash
mcli --http-addr 127.0.0.1:8300 partition-route --rule-name user_rule --key user-100
```

按范围路由（支持逗号分隔多列值）：

```bash
mcli --http-addr 127.0.0.1:8300 partition-route --rule-name user_rule --start 100 --end 200
```

多列示例：

```bash
mcli --http-addr 127.0.0.1:8300 partition-route --rule-name order_rule --key tenant-1,order-100
```

### 8) 复制管理（`repl`）

```bash
mcli --http-addr 127.0.0.1:8300 repl add-node --node-id 4 --host 10.0.0.4 --port 19530 --learner
mcli --http-addr 127.0.0.1:8300 repl promote-learner --node-id 4
mcli --http-addr 127.0.0.1:8300 repl remove-node --node-id 4
mcli --http-addr 127.0.0.1:8301 repl promote --fencing-cmd ./fence-old-primary.sh
mcli --http-addr 127.0.0.1:8301 repl promote --yes
mcli --http-addr 127.0.0.1:8301 repl follow --primary 10.0.0.2:19527 --primary-node-id 2
mcli --http-addr 127.0.0.1:8300 repl status
```

`repl add-node` 默认以 voter 加入，加 `--learner` 则以学习成员加入；`repl promote` 需提供 `--fencing-cmd` 或显式 `--yes`。

### 9) 基础备份管理（`backup`）

需要在 `mudud.cfg` 中配置 `[backup] backup_dir`。

```bash
mcli --http-addr 127.0.0.1:8300 backup create --label daily-1
mcli --http-addr 127.0.0.1:8300 backup list
mcli --http-addr 127.0.0.1:8300 backup verify --label daily-1
mcli --http-addr 127.0.0.1:8300 backup delete --label daily-1
```

### 10) 恢复与时间点恢复（离线）

`restore` 将基础备份与归档 WAL 还原到目标目录；`pitr` 可恢复到指定 `--lsn`、`--time`（毫秒）或 `--xid`。

```bash
mcli restore --label daily-1 --backup-dir /backups --archive-dir /wal-archive --target-dir /restored
mcli pitr --label daily-1 --backup-dir /backups --archive-dir /wal-archive --target-dir /recovered --time 1700000000000
```

### 11) 逻辑导出与恢复

```bash
mcli --http-addr 127.0.0.1:8300 dump --label daily-1 --dir /dumps
mcli --http-addr 127.0.0.1:8300 restore-logical --label daily-1 --dir /dumps
mcli --http-addr 127.0.0.1:8300 restore-logical --label daily-1 --dir /dumps --drop
```

## 参数约束

- `app-detail` 中：
  - `--proc` 必须与 `--module` 一起使用。
- `partition-route` 中：
  - `--key` 与 `--start/--end` 二选一，不可同时使用。
  - 必须提供 `--key`，或至少提供 `--start` / `--end` 之一。

## 返回结果

- 默认输出为格式化 JSON。
- 使用 `--compact` 可输出紧凑 JSON。
- 出错时命令返回非 0，并在 stderr 输出错误原因。
