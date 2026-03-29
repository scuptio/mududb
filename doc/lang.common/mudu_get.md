<!--
quote_begin
content="[Get API](../../sys_interface/src/sync_api.rs#L1)"
lang="rust"
-->
```rust
// sync_api
pub fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    /* ... */
}

// async_api
pub async fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    /* ... */
}
```
<!--quote_end-->
