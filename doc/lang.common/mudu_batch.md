<!--
quote_begin
content="[Batch API](../../sys_interface/src/sync_api.rs#L1)"
lang="rust"
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

`mudu_batch` uses the same argument and return contract as `mudu_command`.

- `oid`: current session ID
- `sql`: SQL text to run as a batch
- `params`: currently must be empty for the libsql-backed batch syscall
- return value: affected row count delta reported by the underlying connection
