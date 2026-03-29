## System Calls in Mudu Procedures

Mudu procedures can call three groups of system APIs:

- Session management syscalls
- SQL APIs for relational CRUD operations
- KV APIs for session-scoped key-value access

The stable syscall entry points are:

- `sys_interface::sync_api` for synchronous/native handwritten procedure code
- `sys_interface::async_api` for async generated/component procedure code

`sys_interface::api` is kept only as a compatibility re-export layer.

## Session Management Syscalls

### 1. `open`

Open a system session and return its `OID`.

<!--
quote_begin
content="[Open API](../lang.common/mudu_open.md#L-L)"
-->
```rust
// sync_api
pub fn mudu_open() -> RS<OID> {
    /* ... */
}

// async_api
pub async fn mudu_open() -> RS<OID> {
    /* ... */
}
```
<!--quote_end-->

### 2. `close`

Close a system session.

<!--
quote_begin
content="[Close API](../lang.common/mudu_close.md#L-L)"
-->
```rust
// sync_api
pub fn mudu_close(session_id: OID) -> RS<()> {
    /* ... */
}

// async_api
pub async fn mudu_close(session_id: OID) -> RS<()> {
    /* ... */
}
```
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
```rust
// sync_api
pub fn mudu_query<R: Entity>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    /* ... */
}

// async_api
pub async fn mudu_query<R: Entity>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    /* ... */
}
```
<!--quote_end-->

`query` performs R2O (relation to object) mapping automatically, returning a result set of objects implementing the
`Entity` trait.

### 2. `command`

`command` for `INSERT` / `UPDATE` / `DELETE`.

<!--
quote_begin
content="[Command API](../lang.common/mudu_command.md#L-L)"
-->
```rust
// sync_api
pub fn mudu_command(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    /* ... */
}

// async_api
pub async fn mudu_command(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    /* ... */
}
```
<!--quote_end-->

### 3. `batch`

`batch` executes a batch SQL string through the batch syscall path. It currently targets libsql-backed runtime
sessions and reuses the same serialized argument and return format as `command`.

<!--
quote_begin
content="[Batch API](../lang.common/mudu_batch.md#L-L)"
-->
```rust
// sync_api
pub fn mudu_batch(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    /* ... */
}

// async_api
pub async fn mudu_batch(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    /* ... */
}
```
<!--quote_end-->

### Parameters for Both

#### oid

Object ID of the current system session.

#### sql

SQL statement with `?` as parameter placeholders.

#### params

Parameter list. For `batch`, the current host implementation requires an empty parameter list because
`libsql::execute_batch` executes raw SQL text.

## KV APIs

### 1. `get`

Read a value by key from the current system session.

<!--
quote_begin
content="[Get API](../lang.common/mudu_get.md#L-L)"
-->
```rust
// sync_api
pub fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    /* ... */
}

// async_api
pub async fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    /* ... */
}
```
<!--quote_end-->

### 2. `put`

Write a key-value pair into the current system session. The underlying syscall name is `mudu_put`.

<!--
quote_begin
content="[Put API](../lang.common/mudu_put.md#L-L)"
-->
```rust
// sync_api
pub fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    /* ... */
}

// async_api
pub async fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    /* ... */
}
```
<!--quote_end-->

### 3. `range`

Scan key-value pairs in the current system session within a key range.

<!--
quote_begin
content="[Range API](../lang.common/mudu_range.md#L-L)"
-->
```rust
// sync_api
pub fn mudu_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    /* ... */
}

// async_api
pub async fn mudu_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    /* ... */
}
```
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
