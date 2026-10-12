use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_contract::tuple::tuple_field::TupleField;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;

/// Query execution contract: a cursor over result tuples.
#[async_trait]
pub trait QueryExec: Send + Sync {
    /// Opens the cursor.
    async fn open(&self) -> RS<()>;
    /// Returns the next result tuple, or `None` when the cursor is exhausted.
    async fn next(&self) -> RS<Option<TupleField>>;
    /// The descriptor of the tuples produced by this query.
    fn tuple_desc(&self) -> RS<TupleFieldDesc>;
}
