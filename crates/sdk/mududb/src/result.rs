//! Canonical guest facade `mududb.result`: row-based query results.
//!
//! Mirrors the cross-language canonical surface
//! (`doc/dev/binding_api_surface.md`): the AssemblyScript `ResultSet`/`Row`
//! semantics exactly — the cursor starts before the first row, `next()`
//! advances it, and `current_row()` is valid only after a successful
//! `next()`. The host drains all rows into the first query response, so
//! iteration never goes back to the wire.

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_type::data_value::DataValue;
use std::sync::Arc;

/// In-memory result set of a `query` call.
pub struct ResultSet {
    columns: Arc<Vec<String>>,
    rows: Vec<TupleValue>,
    /// 0 means "before the first row"; after a successful `next()` it holds
    /// the 1-based index of the current row.
    cursor: usize,
}

impl ResultSet {
    pub(crate) fn from_parts(desc: Arc<TupleFieldDesc>, rows: Vec<TupleValue>) -> ResultSet {
        let columns = desc.fields().iter().map(|f| f.name().to_string()).collect();
        ResultSet {
            columns: Arc::new(columns),
            rows,
            cursor: 0,
        }
    }

    /// Advance the cursor; returns `false` once the rows are exhausted.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> bool {
        if self.cursor < self.rows.len() {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    /// The row under the cursor; an error before the first `next()` and
    /// after the rows are exhausted.
    pub fn current_row(&self) -> RS<Row> {
        if self.cursor == 0 || self.cursor > self.rows.len() {
            return Err(mudu_error!(ErrorCode::InvalidArgument, "no current row"));
        }
        Ok(Row {
            columns: self.columns.clone(),
            values: self.rows[self.cursor - 1].values().to_vec(),
        })
    }

    /// Number of columns in every row.
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    /// Name of the `column`-th column (0-based); an error when out of range.
    pub fn column_name(&self, column: usize) -> RS<&str> {
        self.columns
            .get(column)
            .map(|s| s.as_str())
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "column index is out of range"))
    }

    /// Index of the column called `name`; an error when it does not exist.
    pub fn find_column(&self, name: &str) -> RS<usize> {
        self.columns.iter().position(|c| c == name).ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidArgument,
                format!("column name not found: {}", name)
            )
        })
    }

    /// Whether the cursor has passed the last row.
    pub fn eof(&self) -> bool {
        self.cursor >= self.rows.len()
    }
}

/// A single row of a [`ResultSet`]; values are reachable by index or by
/// column name.
pub struct Row {
    columns: Arc<Vec<String>>,
    values: Vec<DataValue>,
}

impl Row {
    /// Whether the value at `column` (0-based) is NULL; an error when out of
    /// range.
    pub fn is_null(&self, column: usize) -> RS<bool> {
        Ok(self.value(column)?.is_null())
    }

    /// Whether the value in the column called `name` is NULL.
    pub fn is_null_by_name(&self, name: &str) -> RS<bool> {
        self.is_null(self.find_column(name)?)
    }

    /// The value at `column` (0-based); an error when out of range.
    pub fn value(&self, column: usize) -> RS<&DataValue> {
        self.values
            .get(column)
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "column index is out of range"))
    }

    /// The value in the column called `name`; an error when it does not
    /// exist.
    pub fn value_by_name(&self, name: &str) -> RS<&DataValue> {
        self.value(self.find_column(name)?)
    }

    fn find_column(&self, name: &str) -> RS<usize> {
        self.columns.iter().position(|c| c == name).ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidArgument,
                format!("column name not found: {}", name)
            )
        })
    }
}
