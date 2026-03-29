<!--
quote_begin
content="[Query API](../../sys_interface/src/sync_api.rs#L1)"
lang="rust"
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
