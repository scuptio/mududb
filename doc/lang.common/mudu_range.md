<!--
quote_begin
content="[Range API](../../sys_interface/src/api.rs#L215-L221)"
lang="rust"
-->
```rust
pub fn mudu_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    crate::inner_p1::inner_range(session_id, start_key, end_key)
}
```
<!--quote_end-->
