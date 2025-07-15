use crate::tuple::datum::Datum;

pub struct TupleRow {
    data_item:Vec<Datum>,
}


impl TupleRow {
    pub fn new(items:Vec<Datum>) -> TupleRow {
        Self { data_item: items }
    }
    
    pub fn items(&self) -> &Vec<Datum> {
        &self.data_item
    }
    
    pub fn mut_items(&mut self) -> &mut Vec<Datum> {
        &mut self.data_item
    }
    
    pub fn get(&self, n:usize) -> Option<Datum> {
        self.data_item.get(n).cloned()
    }
}

impl AsRef<TupleRow> for TupleRow {
    fn as_ref(&self) -> &Self {
        self
    }
}