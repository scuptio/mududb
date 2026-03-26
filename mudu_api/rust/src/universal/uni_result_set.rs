use crate::universal::uni_tuple_row::UniTupleRow;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UniResultSet {
    pub eof: bool,

    pub row_set: Vec<UniTupleRow>,

    pub cursor: Vec<u8>,
}

impl Default for UniResultSet {
    fn default() -> Self {
        Self {
            eof: Default::default(),

            row_set: Default::default(),

            cursor: Default::default(),
        }
    }
}
