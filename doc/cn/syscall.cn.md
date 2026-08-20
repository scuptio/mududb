## Mudu Procedure 中的系统调用

Mudu Procedure 可以调用四类系统 API：

- 会话管理系统调用
- 用于关系型 CRUD 操作的 SQL API
- 用于会话级键值访问的 KV API
- 用于会话级文件访问的文件系统 API

当前稳定的系统调用入口分为两套：

- `sys_interface::sync_api`：用于同步的原生手写过程代码
- `sys_interface::async_api`：用于异步生成代码或 Component Model 异步路径

`sys_interface::api` 仅作为兼容层保留。

## 会话管理系统调用

### 1. `open`

打开一个系统会话，并返回其 `OID`。

<!--
quote_begin
content="[Open API](../lang.common/mudu_open.md#L-L)"
-->
```rust
// 同步入口
pub fn mudu_open() -> RS<OID> {
    /* ... */
}

// 异步入口
pub async fn mudu_open() -> RS<OID> {
    /* ... */
}
```
<!--quote_end-->

### 2. `close`

关闭一个系统会话。

<!--
quote_begin
content="[Close API](../lang.common/mudu_close.md#L-L)"
-->
```rust
// 同步入口
pub fn mudu_close(session_id: OID) -> RS<()> {
    /* ... */
}

// 异步入口
pub async fn mudu_close(session_id: OID) -> RS<()> {
    /* ... */
}
```
<!--quote_end-->

### 会话管理参数

#### session_id

由 `open` 返回的系统会话 ID。

## SQL API

### 1. `query`

`query` 用于 `SELECT` 语句。

<!--
quote_begin
content="[Query API](../lang.common/mudu_query.md#L-L)"
-->
```rust
// 同步入口
pub fn mudu_query<R: Entity>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    /* ... */
}

// 异步入口
pub async fn mudu_query<R: Entity>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    /* ... */
}
```
<!--quote_end-->

`query` 会自动执行 R2O（relation-to-object，关系到对象）映射，并返回一个由实现 `Entity` trait 的对象组成的结果集。

### 2. `command`

`command` 用于 `INSERT` / `UPDATE` / `DELETE`。

<!--
quote_begin
content="[Command API](../lang.common/mudu_command.md#L-L)"
-->
```rust
// 同步入口
pub fn mudu_command(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    /* ... */
}

// 异步入口
pub async fn mudu_command(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    /* ... */
}
```
<!--quote_end-->

### 3. `batch`

`batch` 通过批量 SQL 系统调用路径执行 SQL 批文本。当前 host 实现下，它在 SQLite、PostgreSQL、
MySQL 的 standalone adapter 路径可用，并复用与 `command` 相同的序列化参数和返回结构。

当前限制：

- `batch` 不支持 SQL 参数
- `mudud` 适配器路径当前仍返回 `NotImplemented`
- 若要跨适配器保持可移植，schema 初始化仍建议逐条使用 `command` 执行

<!--
quote_begin
content="[Batch API](../lang.common/mudu_batch.md#L-L)"
-->
```rust
// 同步入口
pub fn mudu_batch(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    /* ... */
}

// 异步入口
pub async fn mudu_batch(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    /* ... */
}
```
<!--quote_end-->

### 两者通用参数

#### oid

当前系统会话 ID。

#### sql

使用 `?` 作为参数占位符的 SQL 语句。

#### params

参数列表。
对于 `batch`，当前 host 实现要求参数列表为空，因为 `libsql::execute_batch` 执行的是原始 SQL 文本。

## KV API

### 1. `get`

从当前系统会话中按键读取值。

<!--
quote_begin
content="[Get API](../lang.common/mudu_get.md#L-L)"
-->
```rust
// 同步入口
pub fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    /* ... */
}

// 异步入口
pub async fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    /* ... */
}
```
<!--quote_end-->

### 2. `put`

向当前系统会话写入一个键值对。其底层系统调用名为 `mudu_put`。

<!--
quote_begin
content="[Put API](../lang.common/mudu_put.md#L-L)"
-->
```rust
// 同步入口
pub fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    /* ... */
}

// 异步入口
pub async fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    /* ... */
}
```
<!--quote_end-->

### 3. `range`

在当前系统会话中按键范围扫描键值对。

<!--
quote_begin
content="[Range API](../lang.common/mudu_range.md#L-L)"
-->
```rust
// 同步入口
pub fn mudu_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    /* ... */
}

// 异步入口
pub async fn mudu_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    /* ... */
}
```
<!--quote_end-->

### KV API 参数

#### session_id

要操作的系统会话 ID。

#### key

原始键字节序列。

#### value

原始值字节序列。

#### start_key / end_key

`range` 使用的包含式范围边界。

## 文件系统 API

文件系统 API 在文件系统对象（fs object）上提供类 POSIX 的文件操作，共 11 个系统调用：

```rust
// 同步入口（异步入口同名，为 async fn）
pub fn mudu_fs_open(session_id: OID, oid: OID, path: &str, flags: u32) -> RS<u32> {
    /* ... */
}
pub fn mudu_fs_close(session_id: OID, fd: u32) -> RS<()> {
    /* ... */
}
pub fn mudu_fs_read(session_id: OID, fd: u32, len: u32) -> RS<Vec<u8>> {
    /* ... */
}
pub fn mudu_fs_write(session_id: OID, fd: u32, data: &[u8]) -> RS<u32> {
    /* ... */
}
pub fn mudu_fs_pread(session_id: OID, fd: u32, offset: u64, len: u32) -> RS<Vec<u8>> {
    /* ... */
}
pub fn mudu_fs_pwrite(session_id: OID, fd: u32, offset: u64, data: &[u8]) -> RS<()> {
    /* ... */
}
pub fn mudu_fs_lseek(session_id: OID, fd: u32, offset: i64, whence: u32) -> RS<u64> {
    /* ... */
}
pub fn mudu_fs_fstat(session_id: OID, fd: u32) -> RS<FsStat> {
    /* ... */
}
pub fn mudu_fs_stat(session_id: OID, oid: OID, path: &str) -> RS<FsStat> {
    /* ... */
}
pub fn mudu_fs_fsync(session_id: OID, fd: u32) -> RS<()> {
    /* ... */
}
pub fn mudu_fs_readdir(session_id: OID, oid: OID, path: &str) -> RS<Vec<FsDirEntry>> {
    /* ... */
}
```

- `mudu_fs_open` 打开 `oid` 指定的文件系统对象内 `path` 处的文件，返回文件描述符 `fd`；`flags` 取 libc `O_*` 值。
- `mudu_fs_read` / `mudu_fs_write` 在 `fd` 当前游标处读写；`mudu_fs_pread` / `mudu_fs_pwrite` 在指定 `offset` 处读写。
- `mudu_fs_lseek` 移动 `fd` 的游标；`mudu_fs_fsync` 将 `fd` 的写入落盘；`mudu_fs_close` 关闭 `fd`。
- `mudu_fs_fstat` / `mudu_fs_stat` 返回文件元数据 `FsStat`；`mudu_fs_readdir` 列出目录项 `FsDirEntry`（仅目录）。

### 文件系统 API 参数

#### oid

文件系统对象的 OID。

#### path

对象内的相对路径。

#### fd

由 `mudu_fs_open` 返回的文件描述符。

## mtp 同步转异步约束

`mtp rust --async` 会把 `/**mudu-proc**/` 过程中的系统调用自动转换为异步形式：`use` 路径由
`sys_interface::sync_api` 改写为 `sys_interface::async_api`，并在调用点拼接 `.await`。
该转换按被调用函数的裸标识符匹配，因此上述所有系统调用（含 `mudu_fs_*` 家族）必须经过
`use sys_interface::sync_api::{mudu_fs_open, ...}` 导入后以裸名调用，mtp 才能识别并转换；
通过模块路径限定调用（如 `sync_api::mudu_fs_open(...)`）不会被转换。

<!--
quote_begin
content="[KeyTrait](../lang.common/proc_key_traits.md#L-L)"
-->
