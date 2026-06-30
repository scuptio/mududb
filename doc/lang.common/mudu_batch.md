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
- `params`: currently must be empty
- host support:
  SQLite, PostgreSQL, and MySQL standalone adapters implement `batch`
  `mudud` implements batch for empty parameters; batch with parameters currently returns `NotImplemented`
- return value: affected row count delta reported by the underlying connection

> Implementation reference: `mudu_kernel/src/server/worker.rs`, `mudu_kernel/src/server/handlers/batch.rs`, `mudu_contract/src/protocol/mod.rs`.
