//! A set of entities produced by a query.

use crate::database::object::Object;
use crate::database::result_set::ResultSet;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use fallible_iterator::FallibleIterator;
use mudu::common::result::RS;
use mudu::error::MuduError;
use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

/// A query result set whose rows decode into entities of type `R`.
pub struct ObjectSet<R: Object> {
    phantom: PhantomData<R>,
    _desc: Arc<TupleFieldDesc>,
    result_set: Arc<dyn ResultSet>,
}

impl<R: Object> ObjectSet<R> {
    /// Creates an object set from a result set and its row description.
    pub fn new(result_set: Arc<dyn ResultSet>, desc: Arc<TupleFieldDesc>) -> Self {
        Self {
            phantom: PhantomData,
            _desc: desc,
            result_set,
        }
    }

    /// Consume the set and return the raw row source and column description
    /// without decoding any object records. Thin facades that present rows
    /// directly (rather than mapped entities) build on this.
    pub fn into_parts(self) -> (Arc<dyn ResultSet>, Arc<TupleFieldDesc>) {
        (self.result_set, self._desc)
    }
}

impl<R: Object> fmt::Debug for ObjectSet<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ObjectSet").finish_non_exhaustive()
    }
}

impl<R: Object> ObjectSet<R> {
    /// Fetches the next row decoded as an object, or `None` at the end of the set.
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

impl<R: Object + 'static> FallibleIterator for ObjectSet<R> {
    type Item = R;
    type Error = MuduError;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        self.next_record()
    }
}
