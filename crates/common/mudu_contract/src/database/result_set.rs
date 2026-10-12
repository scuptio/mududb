//! Query result set abstraction.

use crate::tuple::tuple_field_desc::TupleFieldDesc;
use crate::tuple::tuple_value::TupleValue;
use async_trait::async_trait;
use mudu::common::result::RS;

/// A synchronous cursor over query result rows.
pub trait ResultSet: Send + Sync {
    /// Fetches the next row, or `None` at the end of the set.
    fn next(&self) -> RS<Option<TupleValue>>;
}

/// An asynchronous cursor over query result rows.
#[async_trait]
pub trait ResultSetAsync: Send + Sync {
    /// Fetches the next row, or `None` at the end of the set.
    async fn next(&self) -> RS<Option<TupleValue>>;

    /// Returns the description of the result row fields.
    fn desc(&self) -> &TupleFieldDesc;
}
