## Mudu Procedure 中的系统调用

Mudu Procedure 可以调用三类系统 API：

- 会话管理系统调用
- 用于关系型 CRUD 操作的 SQL API
- 用于会话级键值访问的 KV API

## 会话管理系统调用

### 1. `open`

打开一个系统会话，并返回其 `OID`。

<!--
quote_begin
content="[Open API](../lang.common/mudu_open.md#L-L)"
-->
<!--
quote_begin
content="[Open API](../../sys_interface/src/api.rs#L91-L93)"
lang="rust"
-->
```rust
pub fn mudu_open() -> RS<OID> {
    crate::inner_p1::inner_open()
}
```
<!--quote_end-->
<!--quote_end-->

### 2. `close`

关闭一个系统会话。

<!--
quote_begin
content="[Close API](../lang.common/mudu_close.md#L-L)"
-->
<!--
quote_begin
content="[Close API](../../sys_interface/src/api.rs#L122-L124)"
lang="rust"
-->
```rust
pub fn mudu_close(session_id: OID) -> RS<()> {
    crate::inner_p1::inner_close(session_id)
}
```
<!--quote_end-->
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
<!--
quote_begin
content="[Query API](../../sys_interface/src/api.rs#L13-L19)"
lang="rust"
-->
```rust
pub fn mudu_query<R: Entity>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    crate::inner_p1::inner_query(oid, sql, params)
}
```
<!--quote_end-->
<!--quote_end-->

`query` 会自动执行 R2O（relation-to-object，关系到对象）映射，并返回一个由实现 `Entity` trait 的对象组成的结果集。

### 2. `command`

`command` 用于 `INSERT` / `UPDATE` / `DELETE`。

<!--
quote_begin
content="[Command API](../lang.common/mudu_command.md#L-L)"
-->
<!--
quote_begin
content="[Command API](../../sys_interface/src/api.rs#L60-L62)"
lang="rust"
-->
```rust
pub fn mudu_command(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    crate::inner_p1::inner_command(oid, sql, params)
}
```
<!--quote_end-->
<!--quote_end-->

### 两者通用参数

#### oid

当前系统会话的对象 ID。

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
<!--
quote_begin
content="[Get API](../../sys_interface/src/api.rs#L153-L155)"
lang="rust"
-->
```rust
pub fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    crate::inner_p1::inner_get(session_id, key)
}
```
<!--quote_end-->
<!--quote_end-->

### 2. `set`

向当前系统会话写入一个键值对。其底层系统调用名为 `mudu_put`。

<!--
quote_begin
content="[Set API](../lang.common/mudu_set.md#L-L)"
-->
<!--
quote_begin
content="[Set API](../../sys_interface/src/api.rs#L184-L186)"
lang="rust"
-->
```rust
pub fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    crate::inner_p1::inner_put(session_id, key, value)
}
```
<!--quote_end-->
<!--quote_end-->

### 3. `range`

在当前系统会话中按键范围扫描键值对。

<!--
quote_begin
content="[Range API](../lang.common/mudu_range.md#L-L)"
-->
<!--
quote_begin
content="[Range API](../../sys_interface/src/api.rs#L215-L221)"
lang="rust"
-->
```rust
pub fn mudu_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    crate::inner_p1::inner_range(session_id, start_key, end_key)
}
```
<!--quote_end-->
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
