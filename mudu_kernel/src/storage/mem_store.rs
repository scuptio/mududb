use async_trait::async_trait;
use mudu::common::buf::Buf;
use mudu::common::id::OID;
use mudu::error::ec::EC as ER;
use scc::HashIndex;
use std::collections::Bound;
use std::sync::Arc;

use crate::contract::data_row::DataRow;
use crate::contract::mem_store::MemStore;
use crate::storage::mem_table::MemTable;
use mudu::common::result::RS;
use mudu::m_error;
use mudu::tuple::tuple_binary_desc::TupleBinaryDesc as TupleDesc;

#[derive(Clone)]
pub struct MemStoreImpl {
    hash: Arc<HashIndex<OID, Arc<MemTable>>>,
}

impl MemStoreImpl {
    pub fn new() -> Self {
        Self {
            hash: Arc::new(HashIndex::new()),
        }
    }

    fn _create_table(&self, oid: OID, key_desc: TupleDesc) -> RS<()> {
        let table = MemTable::new(key_desc);
        let r = self.hash.insert(oid, Arc::new(table));
        r.map_err(|(_, _)| m_error!(ER::ExistingSuchElement))?;
        Ok(())
    }

    fn _drop_table(&self, oid: OID) -> RS<()> {
        let r = self.hash.remove(&oid);
        if !r {
            return Err(m_error!(ER::NoSuchElement));
        }
        Ok(())
    }

    fn _get_key<K: AsRef<[u8]>>(&self, oid: OID, key: K) -> RS<Option<DataRow>> {
        let opt = self.hash.get(&oid);
        match opt {
            Some(e) => e.get().read_key(key),
            None => Err(m_error!(ER::NoSuchElement, format!(
                "table id {} not found in store",
                oid
            ))),
        }
    }

    fn _read_range<K: AsRef<[u8]>>(
        &self,
        oid: OID,
        begin: Bound<K>,
        end: Bound<K>,
    ) -> RS<Vec<DataRow>> {
        let opt = self.hash.get(&oid);
        match opt {
            Some(e) => e.get().read_range(begin, end),
            None => Err(m_error!(ER::NoSuchElement, format!(
                "table id {} not found in store",
                oid
            ))),
        }
    }

    fn _insert_key(&self, oid: OID, key: Buf, row: DataRow) -> RS<Option<(Buf, DataRow)>> {
        let opt = self.hash.get(&oid);
        match opt {
            Some(e) => e.get().insert_key(key, row),
            None => Err(m_error!(ER::NoSuchElement, format!(
                "table id {} not found in store",
                oid
            ))),
        }
    }
}

#[async_trait]
impl MemStore for MemStoreImpl {
    async fn create_table(&self, oid: OID, key_desc: TupleDesc) -> RS<()> {
        self._create_table(oid, key_desc)
    }

    async fn drop_table(&self, oid: OID) -> RS<()> {
        self._drop_table(oid)
    }

    async fn get_key(&self, oid: OID, key: Buf) -> RS<Option<DataRow>> {
        self._get_key(oid, key)
    }

    async fn read_range(&self, oid: OID, begin: Bound<Buf>, end: Bound<Buf>) -> RS<Vec<DataRow>> {
        self._read_range(oid, begin, end)
    }

    async fn insert_key(&self, oid: OID, key: Buf, row: DataRow) -> RS<Option<(Buf, DataRow)>> {
        self._insert_key(oid, key, row)
    }
}

unsafe impl Send for MemStoreImpl {}

unsafe impl Sync for MemStoreImpl {}
