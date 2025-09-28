use crate::sql::proj_field::ProjField;

#[derive(Debug, Clone)]
pub struct ProjList {
    fields: Vec<ProjField>,
}

impl ProjList {
    pub fn new(fields: Vec<ProjField>) -> ProjList {
        Self { fields }
    }

    pub fn fields(&self) -> &Vec<ProjField> {
        &self.fields
    }
}
