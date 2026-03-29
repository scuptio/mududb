use mudu::common::result::RS;
use mudu_contract::database::result_set::ResultSet;
use mudu_contract::tuple::tuple_value::TupleValue;
use std::sync::Mutex;

pub struct LocalResultSet {
    rows: Vec<TupleValue>,
    cursor: Mutex<usize>,
}

impl LocalResultSet {
    pub fn new(rows: Vec<TupleValue>) -> Self {
        Self {
            rows,
            cursor: Mutex::new(0),
        }
    }
}

impl ResultSet for LocalResultSet {
    fn next(&self) -> RS<Option<TupleValue>> {
        let mut cursor = self.cursor.lock().expect("result set cursor lock poisoned");
        if *cursor >= self.rows.len() {
            return Ok(None);
        }
        let row = TupleValue::from(self.rows[*cursor].values().to_vec());
        *cursor += 1;
        Ok(Some(row))
    }
}
