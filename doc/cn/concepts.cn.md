# MuduDB 核心概念

本文解释 MuduDB 文档与工具链中使用的核心概念和术语。

## Mudu Procedure（Mudu 过程）

Mudu Procedure 是运行在 MuduDB 引擎内部、贴近数据执行的用户自定义函数。与传统数据库存储过程语言（如 PL/pgSQL）不同，Mudu Procedure 使用 Rust、AssemblyScript 等通用编程语言编写。

主要特点：

- 使用注释导语标记：Rust 为 `/**mudu-proc**/`，AssemblyScript 为 `/**mudu-proc*/`。
- 第一个参数必须是 `OID`（见下文），表示当前会话/事务上下文。
- 后续参数和返回值使用普通语言类型。
- `mtp` 转译器可将同步源码转换为适合 WASM 运行时的异步生成包装代码。

完整规范见 [procedure.cn.md](procedure.cn.md)。

## OID

**OID** 是 Object Identifier（对象标识符）的缩写。在过程调用中，第一个 `OID` 参数是由内核传入的会话或事务句柄。过程在调用 MuduDB 系统 API（如 `mudu_query`、`mudu_command`、`mudu_get`、`mudu_put`）时需要使用它。

开发者不需要手动创建或解析 OID，它们由运行时在执行过程时自动提供。

## MPK 包

**MPK** 文件（`.mpk`）是 MuduDB 应用包，本质是一个 ZIP 压缩包，包含：

- 应用元数据（名称、版本、语言）。
- `ddl.sql` —— 数据库 schema 定义。
- `init.sql` —— 可选的初始数据。
- 过程描述符（procedure descriptors）。
- 一个或多个编译后的 WebAssembly 组件模块。

使用 `mpk create` 命令创建 MPK，再用 `mcli app-install` 将其安装到运行中的 `mudud` 服务器。

## App、Module 与 Procedure

MPK 安装到 MuduDB 后，内容按以下层次组织：

- **App** —— 顶层应用名称，例如 `wallet`。
- **Module** —— 应用内部的 WebAssembly 组件模块，例如 `wallet`。
- **Procedure** —— 模块中可被调用的导出函数，例如 `transfer_funds`。

调用过程的命令示例：

```bash
mcli --addr 127.0.0.1:9527 app-invoke \
  --app wallet \
  --module wallet \
  --proc transfer_funds \
  --json '{"from_user_id": 1001, "to_user_id": 1002, "amount": 1200}'
```

## 工具链

| 工具 | 作用 | 何时需要 |
|------|------|----------|
| `mudud` | MuduDB 服务器。 | 运行数据库时始终需要。 |
| `mcli` | TCP 协议客户端与 HTTP 管理 CLI。 | 交互式执行 SQL、安装应用包、调用过程。 |
| `mgen` | 源码生成器。根据 SQL DDL 生成 Rust 实体类型。 | 应用有 SQL 表且希望查询结果带类型时。 |
| `mtp` | 转译器。将 Rust/AssemblyScript 源码转换为 Mudu Procedure 格式，并生成异步包装代码。 | 编写或修改 `/**mudu-proc**/` 函数时。 |
| `mpk` | 包构建器。根据 DDL、描述符和 WASM 模块生成 `.mpk` 文件。 | 将应用部署到 `mudud` 之前。 |
| `mudup` | 发布版安装器。下载并激活发布二进制。 | 日常使用或服务器部署，不想从源码构建时。 |

## 系统调用接口

MuduDB 向过程暴露一组精简的系统调用风格 API，供其访问数据库。同步 API 位于 `sys_interface::sync_api`，异步 API 位于 `sys_interface::async_api`。常用调用包括：

- `mudu_query` —— 执行 SELECT 并读取结果。
- `mudu_command` —— 执行 INSERT/UPDATE/DELETE。
- `mudu_get` / `mudu_put` / `mudu_range` —— 键值 API。
- `mudu_batch` —— 批量执行多条语句（在 `mudud` 中目前仅支持空参数）。

每个调用的详细说明见 `doc/lang.common/` 目录。

## 组件模型

MuduDB 使用 [WebAssembly Component Model](https://component-model.bytecodealliance.org/) 运行用户过程。过程被编译为 `wasm32-wasip2` 目标，再与 MuduDB host 组件 compose，最终打包进 MPK。

`mudu_wasm` crate（Cargo 包名为 `mod_0`）提供生成的 guest 绑定和 host 侧转译辅助代码。

## 交互式执行与存储过程执行

同一份 Mudu Procedure 源码可以以两种模式运行：

- **交互式** —— 通过 standalone adapter 直接在 Rust 测试或 benchmark 代码中调用，便于开发和调试。
- **存储过程** —— 打包为 MPK 并安装到 `mudud`，通过 `mcli` 或 TCP 客户端调用。

更多讨论见 [procedure.cn.md](procedure.cn.md)。
