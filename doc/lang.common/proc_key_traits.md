## Key Traits

### SQLStmt

```rust

pub trait SQLStmt: std::fmt::Debug + std::fmt::Display {
    fn to_sql_string(&self) -> String;
}
```

### DatumDyn

<!--
quote_begin
content="[DatumDyn](../../mudu/src/tuple/datum.rs#L22-L34)"
lang="rust"
-->
```rust
pub trait DatumDyn: fmt::Debug + Sync {
    fn dat_type_id_self(&self) -> RS<DatTypeID>;

    fn to_typed(&self, param: &ParamObj) -> RS<DatTyped>;

    fn to_binary(&self, param: &ParamObj) -> RS<DatBinary>;

    fn to_printable(&self, param: &ParamObj) -> RS<DatPrintable>;

    fn to_internal(&self, param: &ParamObj) -> RS<DatInternal>;

    fn clone_boxed(&self) -> Box<dyn DatumDyn>;
}
```
<!--quote_end-->