//! In-memory result set used to materialize query rows from any backend.

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::database::result_set::ResultSet;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_sys::sync::SMutex;

/// A result set that stores all rows locally in a [`Vec`].
pub struct LocalResultSet {
    rows: Vec<TupleValue>,
    cursor: SMutex<usize>,
}

impl LocalResultSet {
    /// Creates a new local result set from the provided rows.
    pub fn new(rows: Vec<TupleValue>) -> Self {
        Self {
            rows,
            cursor: SMutex::new(0),
        }
    }
}

impl ResultSet for LocalResultSet {
    fn next(&self) -> RS<Option<TupleValue>> {
        let mut cursor = self
            .cursor
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "result set cursor lock poisoned"))?;
        if *cursor >= self.rows.len() {
            return Ok(None);
        }
        let row = TupleValue::from(self.rows[*cursor].values().to_vec());
        *cursor += 1;
        Ok(Some(row))
    }
}
