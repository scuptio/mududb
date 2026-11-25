<!--
quote_begin
content="[Query API](../../crates/common/sys_interface/src/sync_api.rs#L1)"
lang="rust"
-->
```rust
// sync_api
pub fn mudu_query<R: Object>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<ObjectSet<R>> {
    /* ... */
}

// async_api
pub async fn mudu_query<R: Object>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<ObjectSet<R>> {
    /* ... */
}
```
<!--quote_end-->
