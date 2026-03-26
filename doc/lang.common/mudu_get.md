<!--
quote_begin
content="[Get API](../../sys_interface/src/api.rs#L153-L155)"
lang="rust"
-->
```rust
pub fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    crate::inner_p1::inner_get(session_id, key)
}
```
<!--quote_end-->
