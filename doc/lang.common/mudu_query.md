<!--
quote_begin
content="[Query API](../../sys_interface/src/api.rs#L34-L40)"
lang="rust"
-->

```rust
pub fn mudu_command(
    xid: XID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<u64> {
    inner::inner_command(xid, sql, params)
}
```

<!--quote_end-->