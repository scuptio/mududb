use mudu_type::data_type::DataType;

#[derive(Debug, Clone)]
pub struct ProjField {
    index: usize,
    name: String,
    type_desc: DataType,
}

impl ProjField {
    pub fn new(index: usize, name: String, type_desc: DataType) -> Self {
        Self {
            index,
            name,
            type_desc,
        }
    }

    pub fn index_of_tuple(&self) -> usize {
        self.index
    }
    pub fn name(&self) -> &String {
        &self.name
    }

    pub fn type_desc(&self) -> &DataType {
        &self.type_desc
    }
}
