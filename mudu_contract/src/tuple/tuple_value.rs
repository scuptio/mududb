use mudu_type::dat_value::DatValue;

pub struct TupleValue {
    value:Vec<DatValue>
}

impl TupleValue {
    pub fn from(value:Vec<DatValue>) -> TupleValue {
        Self {value}
    }

    pub fn values(&self) -> &[DatValue] {
        &self.value
    }

    pub fn into(self) -> Vec<DatValue> {
        self.value
    }
}

impl AsRef<TupleValue> for TupleValue {
    fn as_ref(&self) -> &TupleValue {
        self
    }
}