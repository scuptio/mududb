//! `database::entity_set` module.
#![allow(missing_docs)]

use crate::database::entity::Entity;
use crate::database::entity_utils;
use crate::database::result_set::ResultSet;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use fallible_iterator::FallibleIterator;
use mudu::common::result::RS;
use mudu::error::MuduError;
use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

pub struct RecordSet<R: Entity> {
    phantom: PhantomData<R>,
    _desc: Arc<TupleFieldDesc>,
    result_set: Arc<dyn ResultSet>,
}

impl<R: Entity> RecordSet<R> {
    pub fn new(result_set: Arc<dyn ResultSet>, desc: Arc<TupleFieldDesc>) -> Self {
        Self {
            phantom: PhantomData,
            _desc: desc,
            result_set,
        }
    }
}

impl<R: Entity> fmt::Debug for RecordSet<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RecordSet").finish_non_exhaustive()
    }
}

impl<R: Entity> RecordSet<R> {
    pub fn next_record(&self) -> RS<Option<R>> {
        let opt = self.result_set.next()?;
        if let Some(row) = opt {
            let r = entity_utils::entity_from_tuple_value::<R, _>(&row)?;
            Ok(Some(r))
        } else {
            Ok(None)
        }
    }
}

impl<R: Entity + 'static> FallibleIterator for RecordSet<R> {
    type Item = R;
    type Error = MuduError;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        self.next_record()
    }
}
