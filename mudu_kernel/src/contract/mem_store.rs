use crate::contract::data_row::DataRow;
use async_trait::async_trait;
use mudu::common::buf::Buf;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::tuple::tuple_binary_desc::TupleBinaryDesc as TupleDesc;
use std::ops::Bound;

#[async_trait]
pub trait MemStore: Send + Sync {
    async fn create_table(&self, oid: OID, key_desc: TupleDesc) -> RS<()>;

    async fn drop_table(&self, oid: OID) -> RS<()>;

    async fn get_key(&self, oid: OID, key: Buf) -> RS<Option<DataRow>>;

    async fn read_range(&self, oid: OID, begin: Bound<Buf>, end: Bound<Buf>) -> RS<Vec<DataRow>>;

    async fn insert_key(&self, oid: OID, key: Buf, row: DataRow) -> RS<Option<(Buf, DataRow)>>;
}
