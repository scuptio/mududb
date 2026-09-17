## Key Traits

### SQLStmt

<!--
quote_begin
content="[Entity](../../mudu_contract/src/database/entity.rs#L23-L44)"
lang="rust"
-->
```rust
pub trait Entity: Datum {
    fn tuple_desc() -> &'static TupleFieldDesc;

    fn table_name() -> &'static str;

    fn from_tuple(row: &TupleField) -> RS<Self>;

    fn from_tuple_value(row: &TupleValue) -> RS<Self>;

    fn to_tuple(&self) -> RS<TupleField>;

    fn to_tuple_value(&self) -> RS<TupleValue>;
}
```
<!--quote_end-->


<!--
quote_begin
content="[SQLStmt](../../mudu_contract/src/database/sql_stmt.rs#L6-L10)"
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
content="[DatumDyn](../../mudu_type/src/datum.rs#L20-L40)"
lang="rust"
-->
```rust
pub trait Datum: DatumDyn + Clone + 'static {
    fn data_type() -> DataType;

    fn from_binary(binary: &[u8]) -> RS<Self>;

    fn from_value(value: &DataValue) -> RS<Self>;

    fn from_textual(textual: &str) -> RS<Self>;
}

pub trait DatumDyn: fmt::Debug + Send + Sync + Any {
    fn type_family(&self) -> RS<TypeFamily>;

    fn to_binary(&self, data_type: &DataType) -> RS<DataBinary>;

    fn to_textual(&self, data_type: &DataType) -> RS<DataTextual>;

    fn to_value(&self, data_type: &DataType) -> RS<DataValue>;

    fn clone_boxed(&self) -> Box<dyn DatumDyn>;
}
```
<!--quote_end-->

### FieldChange

`FieldChange<T>` is the per-column change type of the partial-update
changesets (`<Table>Change`) that `mgen entity` generates for each table.
`Unchanged` leaves the column out of the generated `UPDATE ... SET` clause,
`Set(v)` updates it. Nullable columns use `FieldChange<Option<T>>`, so
`Set(None)` writes a SQL `NULL` while `Unchanged` leaves the column
untouched.

<!--
quote_begin
content="[FieldChange](../../mudu_contract/src/database/field_change.rs#L15-L22)"
lang="rust"
-->
```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum FieldChange<T> {
    /// The column is not touched by the update.
    #[default]
    Unchanged,
    /// The column is updated to the contained value.
    Set(T),
}
```
<!--quote_end-->
