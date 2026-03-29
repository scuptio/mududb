<!--
quote_begin
content="[Put API](../../sys_interface/src/sync_api.rs#L1)"
lang="rust"
-->
```rust
// sync_api
pub fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    /* ... */
}

// async_api
pub async fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    /* ... */
}
```
<!--quote_end-->
