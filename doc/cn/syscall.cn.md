## Mudu Procedure 中的系统调用

Mudu Procedure 可以调用三类系统 API：

- 会话管理系统调用
- 用于关系型 CRUD 操作的 SQL API
- 用于会话级键值访问的 KV API

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

### 两者通用参数

#### oid

当前系统会话 ID。

#### sql

使用 `?` 作为参数占位符的 SQL 语句。

#### params

参数列表。

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

<!--
quote_begin
content="[KeyTrait](../lang.common/proc_key_traits.md#L-L)"
-->
