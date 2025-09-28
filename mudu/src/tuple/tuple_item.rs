use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct TupleItem {
    data_item: Vec<Vec<u8>>,
}


impl TupleItem {
    pub fn new(items: Vec<Vec<u8>>) -> TupleItem {
        Self { data_item: items }
    }

    pub fn items(&self) -> &Vec<Vec<u8>> {
        &self.data_item
    }

    pub fn mut_items(&mut self) -> &mut Vec<Vec<u8>> {
        &mut self.data_item
    }

    pub fn get(&self, n: usize) -> Option<Vec<u8>> {
        self.data_item.get(n).cloned()
    }
}

impl AsRef<TupleItem> for TupleItem {
    fn as_ref(&self) -> &Self {
        self
    }
}