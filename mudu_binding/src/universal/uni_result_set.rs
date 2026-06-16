use crate::universal::uni_tuple_row::UniTupleRow;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[derive(Default)]
pub struct UniResultSet {
    pub eof: bool,

    pub row_set: Vec<UniTupleRow>,

    pub cursor: Vec<u8>,
}

