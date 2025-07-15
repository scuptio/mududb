use crate::common::result::RS;
use crate::database::record::Record;
use crate::database::result_set::ResultSet;
use crate::database::row_desc::RowDesc;
use std::marker::PhantomData;
use std::sync::Arc;

pub struct RecordSet<R:Record> {
    phantom:PhantomData<R>,
    desc:RowDesc,
    result_set:Arc<dyn ResultSet>,
}

impl <R:Record> RecordSet<R> {
    pub fn new(result_set:Arc<dyn ResultSet>, desc:RowDesc) -> Self {
        Self {phantom:PhantomData,desc,result_set }
    }
}

impl <R:Record> RecordSet<R> {
    pub fn next(&self) -> RS<Option<R>> {
        let opt = self.result_set.next()?;
        if let Some(row) = opt {
            let r = R::from_tuple(&row, &self.desc)?;
            Ok(Some(r))
        } else {
            Ok(None)
        }
    }
}