# 你的第一个 MPK

本教程以现有的 `example/wallet` 项目为例，带你完成第一个 MuduDB 应用包（`.mpk`）的构建与安装。学完后，你会了解如何将编译好的 WebAssembly 过程打包并安装到 `mudud` 中，然后通过 `mcli` 调用。

## 你将学到什么

- MPK 包里包含什么（DDL、描述符和 WASM 模块）。
- 如何使用 `mpm-build` 构建 `.mpk`。
- 如何使用 `mpm-install` 将 `.mpk` 安装到运行中的 MuduDB 服务器。
- 如何使用 `mcli` 验证安装并调用过程。

## 前置条件

先按照 [`how_to_start.cn.md`](how_to_start.cn.md) 完成环境配置，并确保 `mpm-build`、`mpm-install`、`mudud`、`mcli` 都在 `PATH` 中。

## 1. wallet 示例

`example/wallet` 是一个完整的 Rust 项目，定义了 `create_user`、`deposit`、`transfer_funds` 等 Mudu Procedure。源码位于 `example/wallet/src/rust/procedures.rs`，schema 位于 `example/wallet/sql/ddl.sql`。

如果你想了解如何手写 procedure，请参阅 [`procedure.cn.md`](procedure.cn.md)。本教程重点介绍如何打包和安装已有项目。

## 2. 构建应用包

在 `example/wallet` 目录下执行：

```bash
cargo make package
```

这一条命令会跑完整流程：

1. 从源码重新安装工作区 CLI 工具（`mgen`、`mtp`、`mpm-build` 等），确保工具链与当前 commit 保持一致。
2. `mgen entity ...` —— 根据 `sql/ddl.sql` 生成带类型的 Rust 实体绑定。
3. 运行 `mtp`，将同步过程代码转译为异步 WebAssembly 包装代码。
4. 使用 `cargo fmt` 格式化生成的源码。
5. `cargo build --target wasm32-wasip2 --release` —— 将生成代码编译为 WebAssembly 组件。
6. `mpm-build create ...` —— 将 DDL、描述符和 WASM 文件打包为 `target/wasm32-wasip2/release/wallet.mpk`。

如果你已经拥有 WASM 文件和描述符，也可以直接调用 `mpm-build create`；`cargo make package` 只是示例项目提供的便捷封装。

## 3. 启动服务器

在一个终端启动 `mudud`：

```bash
ulimit -n 65535
mudud
```

## 4. 安装应用包

在另一个终端使用 `mpm-install` 安装 `.mpk`：

```bash
mpm-install target/wasm32-wasip2/release/wallet.mpk
```

默认情况下，`mpm-install` 会将包发送到 `127.0.0.1:8300` 的 MuduDB HTTP 管理端口。你可以用 `--server` 指定其他服务器：

```bash
mpm-install --server 192.168.1.100:8300 target/wasm32-wasip2/release/wallet.mpk
```

## 5. 确认安装成功

```bash
mcli --http-addr 127.0.0.1:8300 app-list
mcli --http-addr 127.0.0.1:8300 app-detail --app wallet
```

## 6. 调用过程

创建两个用户：

```bash
mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke \
  --app wallet --module wallet --proc create_user \
  --json '{"user_id": 1001, "name": "Alice", "email": "alice@example.com"}'

mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke \
  --app wallet --module wallet --proc create_user \
  --json '{"user_id": 1002, "name": "Bob", "email": "bob@example.com"}'
```

充值并转账：

```bash
mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke \
  --app wallet --module wallet --proc deposit \
  --json '{"user_id": 1001, "amount": 5000}'

mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke \
  --app wallet --module wallet --proc transfer_funds \
  --json '{"from_user_id": 1001, "to_user_id": 1002, "amount": 1200}'
```

查询余额：

```bash
mcli --addr 127.0.0.1:9527 shell --app wallet
```

```sql
SELECT user_id, balance FROM wallets WHERE user_id = 1001;
SELECT user_id, balance FROM wallets WHERE user_id = 1002;
\q
```

## 下一步

- 阅读 [`concepts.cn.md`](concepts.cn.md) 了解上述术语。
- 阅读 [`procedure.cn.md`](procedure.cn.md) 学习如何编写自己的 procedure。
- 尝试 [`example/key-value`](../example/key-value/README.md) 示例，它展示了更小巧的键值 API 用法。
