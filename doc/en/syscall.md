## System Calls in Mudu Procedures

Mudu procedures can call three groups of system APIs:

- Session management syscalls
- SQL APIs for relational CRUD operations
- KV APIs for session-scoped key-value access

## Session Management Syscalls

### 1. `open`

Open a system session and return its `OID`.

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

Close a system session.

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

### Parameters for Session Management

#### session_id

System session ID returned by `open`.

## SQL APIs

### 1. `query`

`query` for `SELECT` statements.

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

`query` performs R2O (relation to object) mapping automatically, returning a result set of objects implementing the
`Entity` trait.

### 2. `command`

`command` for `INSERT` / `UPDATE` / `DELETE`.

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

### Parameters for Both

#### oid

Object ID of the current system session.

#### sql

SQL statement with `?` as parameter placeholders.

#### params

Parameter list.

## KV APIs

### 1. `get`

Read a value by key from the current system session.

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

Write a key-value pair into the current system session. The underlying syscall name is `mudu_put`.

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

Scan key-value pairs in the current system session within a key range.

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

### Parameters for KV APIs

#### session_id

System session ID to operate on.

#### key

Raw key bytes.

#### value

Raw value bytes.

#### start_key / end_key

Inclusive range boundaries used by `range`.

<!--
quote_begin
content="[KeyTrait](../lang.common/proc_key_traits.md#L-L)"
-->
