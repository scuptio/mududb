use crate::contract::data_row::DataRow;
use mudu::common::buf::Buf;
use mudu::common::result::RS;
use mudu::tuple::tuple_binary_desc::TupleBinaryDesc as TupleDesc;
use mudu::tuple::tuple_key::{TupleKey, _KeyRef};
use scc::ebr::Guard;
use scc::TreeIndex;
use std::collections::Bound;
use std::sync::Arc;

pub struct MemTable {
    inner: Arc<MemTableI>,
}

struct MemTableI {
    key_desc: TupleDesc,
    tree_index: TreeIndex<TupleKey, DataRow>,
}

impl MemTable {
    pub fn new(key_desc: TupleDesc) -> Self {
        Self {
            inner: Arc::new(MemTableI::new(key_desc)),
        }
    }

    pub fn read_key<K: AsRef<[u8]>>(&self, key: K) -> RS<Option<DataRow>> {
        self.inner.read_key(key)
    }

    pub fn read_range<K: AsRef<[u8]>>(&self, begin: Bound<K>, end: Bound<K>) -> RS<Vec<DataRow>> {
        self.inner.read_range(begin, end)
    }

    pub fn insert_key(&self, key: Buf, row: DataRow) -> RS<Option<(Buf, DataRow)>> {
        self.inner.insert_key(key, row)
    }
}
impl MemTableI {
    pub fn new(key_desc: TupleDesc) -> Self {
        Self {
            key_desc,
            tree_index: TreeIndex::new(),
        }
    }

    pub fn read_key<K: AsRef<[u8]>>(&self, key: K) -> RS<Option<DataRow>> {
        let key_ref = _KeyRef::new(&key);
        let g = Guard::new();
        todo!();
        /*
        let opt_r = self.tree_index.peek(todo!(), &g);
        let r = opt_r.cloned();
        Ok(r)
         */
    }

    pub fn read_range<K: AsRef<[u8]>>(&self, begin: Bound<K>, end: Bound<K>) -> RS<Vec<DataRow>> {
        todo!()
        /*
       let mut rows = vec![];
       let g = Guard::new();
       let begin_bound = begin.map(|k| _Key::new(k));
       let end_bound = end.map(|k| _Key::new(k));

       let mut range = self.tree_index.range((begin_bound, end_bound), &g);
       loop {
           let opt = range.next();
           match opt {
               Some((_k, v)) => rows.push(v.clone()),
               None => {
                   break;
               }
           }
       }
       Ok(rows)
       */
    }

    pub fn insert_key(&self, key: Buf, row: DataRow) -> RS<Option<(Buf, DataRow)>> {
        let key = TupleKey::from_buf(&self.key_desc, key);
        let r = self.tree_index.insert(key, row);
        match r {
            Ok(()) => Ok(None),
            Err((k, v)) => Ok(Some((k.into(), v))),
        }
    }
}
