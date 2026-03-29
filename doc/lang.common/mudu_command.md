<!--
quote_begin
content="[Command API](../../sys_interface/src/sync_api.rs#L1)"
lang="rust"
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
