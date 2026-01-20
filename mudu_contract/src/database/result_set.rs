use async_trait::async_trait;
use mudu::common::result::RS;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use crate::tuple::tuple_value::TupleValue;

pub trait ResultSet: Send + Sync {
    fn next(&self) -> RS<Option<TupleValue>>;
}

#[async_trait]
pub trait ResultSetAsync: Send + Sync {
    async fn next(&self) -> RS<Option<TupleValue>>;

    fn desc(&self) -> &TupleFieldDesc;
}