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
