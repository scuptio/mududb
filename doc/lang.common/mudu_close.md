<!--
quote_begin
content="[Close API](../../sys_interface/src/sync_api.rs#L1)"
lang="rust"
-->
```rust
// sync_api
pub fn mudu_close(session_id: OID) -> RS<()> {
    /* ... */
}

// async_api
pub async fn mudu_close(session_id: OID) -> RS<()> {
    /* ... */
}
```
<!--quote_end-->
