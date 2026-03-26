<!--
quote_begin
content="[Set API](../../sys_interface/src/api.rs#L184-L186)"
lang="rust"
-->
```rust
pub fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    crate::inner_p1::inner_put(session_id, key, value)
}
```
<!--quote_end-->
