use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::tuple::tuple_item::TupleItem;
use mudu::tuple::tuple_item_desc::TupleItemDesc;

#[async_trait]
pub trait QueryExec: Send + Sync {
    async fn open(&self) -> RS<()>;
    async fn next(&self) -> RS<Option<TupleItem>>;
    fn tuple_desc(&self) -> RS<TupleItemDesc>;
}
