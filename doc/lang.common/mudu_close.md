<!--
quote_begin
content="[Close API](../../sys_interface/src/api.rs#L122-L124)"
lang="rust"
-->
```rust
pub fn mudu_close(session_id: OID) -> RS<()> {
    crate::inner_p1::inner_close(session_id)
}
```
<!--quote_end-->
