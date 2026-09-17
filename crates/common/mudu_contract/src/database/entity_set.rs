//! `database::entity_set` module.
#![allow(missing_docs)]

use crate::database::entity::Entity;
use crate::database::result_set::ResultSet;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use fallible_iterator::FallibleIterator;
use mudu::common::result::RS;
use mudu::error::MuduError;
use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

pub struct EntitySet<R: Entity> {
    phantom: PhantomData<R>,
    _desc: Arc<TupleFieldDesc>,
    result_set: Arc<dyn ResultSet>,
}

impl<R: Entity> EntitySet<R> {
    pub fn new(result_set: Arc<dyn ResultSet>, desc: Arc<TupleFieldDesc>) -> Self {
        Self {
            phantom: PhantomData,
            _desc: desc,
            result_set,
        }
    }

    /// Consume the set and return the raw row source and column description
    /// without decoding any entity records. Thin facades that present rows
    /// directly (rather than mapped entities) build on this.
    pub fn into_parts(self) -> (Arc<dyn ResultSet>, Arc<TupleFieldDesc>) {
        (self.result_set, self._desc)
    }
}

impl<R: Entity> fmt::Debug for EntitySet<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EntitySet").finish_non_exhaustive()
    }
}

impl<R: Entity> EntitySet<R> {
    pub fn next_record(&self) -> RS<Option<R>> {
        let opt = self.result_set.next()?;
        if let Some(row) = opt {
            let r = R::from_tuple_value(&row)?;
            Ok(Some(r))
        } else {
            Ok(None)
        }
    }
}

impl<R: Entity + 'static> FallibleIterator for EntitySet<R> {
    type Item = R;
    type Error = MuduError;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        self.next_record()
    }
}
