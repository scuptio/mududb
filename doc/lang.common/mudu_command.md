<!--
quote_begin
content="[Command API](../../sys_interface/src/api.rs#L11-L19)"
lang="rust"
-->

```rust
pub fn mudu_query<
    R: Entity
>(
    xid: XID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    inner::inner_query(xid, sql, params)
}
```

<!--quote_end-->