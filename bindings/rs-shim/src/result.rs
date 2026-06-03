use std::cell::RefCell;

use crate::error;
use crate::exports::mududb::component_shim::{system, types};
use crate::value;

#[derive(Clone, Default)]
pub struct ResultSet {
    columns: Vec<String>,
    rows: Vec<Row>,
    cursor: RefCell<usize>,
}

#[derive(Clone, Default)]
pub struct Row {
    columns: Vec<String>,
    values: Vec<types::Value>,
}

impl ResultSet {
    pub fn new(columns: Vec<String>, values: Vec<Vec<types::Value>>) -> Self {
        let rows = values
            .into_iter()
            .map(|values| Row {
                columns: columns.clone(),
                values,
            })
            .collect();
        Self {
            columns,
            rows,
            cursor: RefCell::new(0),
        }
    }

    pub fn from_facade(
        batch: mududb::contract::database::result_batch::ResultBatch,
        desc: mududb::contract::tuple::tuple_field_desc::TupleFieldDesc,
    ) -> Self {
        let columns = desc
            .fields()
            .iter()
            .map(|field| field.name().to_string())
            .collect::<Vec<_>>();
        let rows = batch
            .into_rows()
            .into_iter()
            .map(|row| {
                row.values()
                    .iter()
                    .map(value::from_dat_value)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        Self::new(columns, rows)
    }
}

impl system::GuestResultSet for ResultSet {
    fn next(&self) -> Result<bool, types::Error> {
        let mut cursor = self.cursor.borrow_mut();
        if *cursor < self.rows.len() {
            *cursor += 1;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn current_row(&self) -> Result<Option<system::Row>, types::Error> {
        let cursor = *self.cursor.borrow();
        if cursor == 0 || cursor > self.rows.len() {
            return Ok(None);
        }
        Ok(Some(system::Row::new(self.rows[cursor - 1].clone())))
    }

    fn column_count(&self) -> Result<u32, types::Error> {
        Ok(self.columns.len() as u32)
    }

    fn column_name(&self, column: u32) -> Result<String, types::Error> {
        self.columns
            .get(column as usize)
            .cloned()
            .ok_or_else(|| error::range("column index is out of range"))
    }

    fn find_column(&self, name: String) -> Result<Option<u32>, types::Error> {
        Ok(self
            .columns
            .iter()
            .position(|column| column == &name)
            .map(|index| index as u32))
    }

    fn eof(&self) -> Result<bool, types::Error> {
        Ok(*self.cursor.borrow() >= self.rows.len())
    }
}

impl system::GuestRow for Row {
    fn is_null(&self, column: u32) -> Result<bool, types::Error> {
        Ok(matches!(
            self.values.get(column as usize),
            Some(types::Value::Null)
        ))
    }

    fn is_null_by_name(&self, name: String) -> Result<bool, types::Error> {
        let Some(index) = self.columns.iter().position(|column| column == &name) else {
            return Err(error::range("column name not found"));
        };
        self.is_null(index as u32)
    }

    fn value(&self, column: u32) -> Result<types::Value, types::Error> {
        self.values
            .get(column as usize)
            .cloned()
            .ok_or_else(|| error::range("column index is out of range"))
    }

    fn value_by_name(&self, name: String) -> Result<types::Value, types::Error> {
        let Some(index) = self.columns.iter().position(|column| column == &name) else {
            return Err(error::range("column name not found"));
        };
        self.value(index as u32)
    }
}
