## Key Traits

### SQLStmt

<!--
quote_begin
content="[Entity](../../mudu/src/database/entity.rs#L12-L34)"
lang="rust"
-->

```rust
pub trait Entity: private::Sealed + Datum {
    fn new_empty() -> Self;

    fn tuple_desc() -> &'static TupleFieldDesc;

    fn object_name() -> &'static str;

    fn get_field_binary(&self, field_name: &str) -> RS<Option<Vec<u8>>>;

    fn set_field_binary<B: AsRef<[u8]>>(&mut self, field_name: &str, binary: B) -> RS<()>;

    fn get_field_value(&self, field_name: &str) -> RS<Option<DatValue>>;

    fn set_field_value<D: AsRef<DatValue>>(&mut self, field_name: &str, value: D) -> RS<()>;

    fn from_tuple(tuple_row: &TupleField) -> RS<Self> {
        entity_utils::entity_from_tuple(tuple_row)
    }

    fn to_tuple(&self) -> RS<TupleField> {
        entity_utils::entity_to_tuple(self)
    }
}
```

<!--quote_end-->


<!--
quote_begin
content="[SQLStmt](../../mudu/src/database/sql_stmt.rs#L3-L8)"
lang="rust"
-->

```rust
pub trait SQLStmt: fmt::Debug + fmt::Display + Sync + Send {
    fn to_sql_string(&self) -> String;

    fn clone_boxed(&self) -> Box<dyn SQLStmt>;
}
```

<!--quote_end-->

### Datum, DatumDyn

<!--
quote_begin
content="[DatumDyn](../../mudu/src/data_type/datum.rs#L18-L38)"
lang="rust"
-->

```rust
pub trait Datum: DatumDyn + Clone + 'static {
    fn dat_type() -> &'static DatType;

    fn from_binary(binary: &[u8]) -> RS<Self>;

    fn from_value(value: &DatValue) -> RS<Self>;

    fn from_textual(textual: &str) -> RS<Self>;
}

pub trait DatumDyn: fmt::Debug + Send + Sync + Any {
    fn dat_type_id(&self) -> RS<DatTypeID>;

    fn to_binary(&self, dat_type: &DatType) -> RS<DatBinary>;

    fn to_textual(&self, dat_type: &DatType) -> RS<DatTextual>;

    fn to_value(&self, dat_type: &DatType) -> RS<DatValue>;

    fn clone_boxed(&self) -> Box<dyn DatumDyn>;
}
```

<!--quote_end-->
