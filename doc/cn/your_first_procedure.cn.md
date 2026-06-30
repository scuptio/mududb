# 你的第一个 Mudu Procedure

本教程以现有的 `example/wallet` 项目为例，带你走一遍 Mudu Procedure 的完整生命周期。学完后，你会了解手写 Rust 过程如何变成 `mudud` 中可安装、可调用的应用包。

## 你将学到什么

- 如何将一个 Rust 函数标记为 Mudu Procedure。
- `mgen` 如何根据 SQL DDL 生成带类型的实体绑定。
- `mtp` 如何将同步过程代码转译为异步 WebAssembly 包装代码。
- `mpk` 如何将 schema、描述符和 WASM 模块打包。
- 如何使用 `mcli` 安装并调用应用包。

## 前置条件

先按照 [`how_to_start.cn.md`](how_to_start.cn.md) 完成环境配置，并确保 `mgen`、`mtp`、`mpk`、`mudud`、`mcli` 都在 `PATH` 中。

## 1. 手写的过程

打开 `example/wallet/src/rust/procedures.rs`。`transfer_funds` 函数就是一个典型的 Mudu Procedure：

```rust
/**mudu-proc**/
pub fn transfer_funds(xid: OID, from_user_id: i32, to_user_id: i32, amount: i32) -> RS<()> {
    // ... 使用 mudu_query / mudu_command 的业务逻辑 ...
}
```

要点：

- `/**mudu-proc**/` 注释导语告诉 transpiler 导出这个函数。
- 第一个参数 `xid: OID` 是运行时传入的会话/事务上下文。
- 后续参数使用普通 Rust 类型（如 `i32`）。
- 返回类型为 `RS<()>`（即 `Result<(), MuduError>` 的别名）。

## 2. 数据库 schema

wallet 的 schema 位于 `example/wallet/sql/ddl.sql`，例如：

```sql
CREATE TABLE wallets
(
    user_id    INT PRIMARY KEY,
    balance    INT,
    updated_at INT
);
```

`mgen` 读取该文件，在 `example/wallet/src/generated/` 中生成带类型的 Rust 实体（如 `Wallets`）。这些类型用于 `mudu_query::<Wallets>(...)` 解码结果行。

## 3. 构建应用包

在 `example/wallet` 目录下执行：

```bash
cargo make package
```

这一条命令会跑完整流程：

1. `mgen entity ...` —— 根据 `sql/ddl.sql` 生成 `src/generated/` 实体类型和 `package/type.desc.json`。
2. `python ../../script/build/transpiler.py ...` —— 运行 `mtp`，将 `src/rust/procedures.rs` 转译为 `src/generated/procedures.rs` 中的异步包装代码，并生成 `package/package.desc.json`。
3. `cargo build --target wasm32-wasip2 --release` —— 将生成代码编译为 WebAssembly 组件。
4. `mpk create ...` —— 将 DDL、描述符和 WASM 文件打包为 `target/wasm32-wasip2/release/wallet.mpk`。

## 4. 启动服务器并安装包

在一个终端启动 `mudud`：

```bash
ulimit -n 65535
mudud
```

在另一个终端安装包：

```bash
mcli --http-addr 127.0.0.1:8300 app-install \
  --mpk target/wasm32-wasip2/release/wallet.mpk
```

确认安装成功：

```bash
mcli --http-addr 127.0.0.1:8300 app-list
mcli --http-addr 127.0.0.1:8300 app-detail --app wallet
```

## 5. 调用过程

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
SELECT user_id, balance FROM wallets WHERE user_id IN (1001, 1002);
\q
```

## 下一步

- 阅读 [`concepts.cn.md`](concepts.cn.md) 了解上述术语。
- 尝试 [`example/key-value`](../example/key-value/README.md) 示例，它展示了更小巧的键值 API 用法。
- 仿照 `example/wallet` 创建你自己的项目，或修改现有示例来编写新 procedure。
