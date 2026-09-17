use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_contract::tuple::tuple_field::TupleField;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;

#[async_trait]
pub trait QueryExec: Send + Sync {
    async fn open(&self) -> RS<()>;
    async fn next(&self) -> RS<Option<TupleField>>;
    fn tuple_desc(&self) -> RS<TupleFieldDesc>;
}
