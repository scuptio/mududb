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
