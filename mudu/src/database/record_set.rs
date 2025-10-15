use crate::common::result::RS;
use crate::database::record::Record;
use crate::database::result_set::ResultSet;
use crate::error::err::MError;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use fallible_iterator::FallibleIterator;
use std::marker::PhantomData;
use std::sync::Arc;

pub struct RecordSet<R: Record> {
    phantom: PhantomData<R>,
    desc: Arc<TupleFieldDesc>,
    result_set: Arc<dyn ResultSet>,
}

impl<R: Record> RecordSet<R> {
    pub fn new(result_set: Arc<dyn ResultSet>, desc: Arc<TupleFieldDesc>) -> Self {
        Self { phantom: PhantomData, desc, result_set }
    }
}

impl<R: Record> RecordSet<R> {
    pub fn next_record(&self) -> RS<Option<R>> {
        let opt = self.result_set.next()?;
        if let Some(row) = opt {
            let r = R::from_tuple(row, &(*self.desc))?;
            Ok(Some(r))
        } else {
            Ok(None)
        }
    }
}

impl<R: Record + 'static> FallibleIterator for RecordSet<R> {
    type Item = R;
    type Error = MError;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        self.next_record()
    }
}