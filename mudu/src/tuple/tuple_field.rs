use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct TupleField {
    fields: Vec<Vec<u8>>,
}

impl TupleField {
    pub fn new(fields: Vec<Vec<u8>>) -> TupleField {
        Self { fields }
    }

    pub fn fields(&self) -> &Vec<Vec<u8>> {
        &self.fields
    }

    pub fn mut_fields(&mut self) -> &mut Vec<Vec<u8>> {
        &mut self.fields
    }

    pub fn get(&self, n: usize) -> Option<Vec<u8>> {
        self.fields.get(n).cloned()
    }
}

impl AsRef<TupleField> for TupleField {
    fn as_ref(&self) -> &Self {
        self
    }
}
