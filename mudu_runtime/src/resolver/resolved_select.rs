use mudu::tuple::datum_desc::DatumDesc;

use crate::resolver::filter::Filter;

pub struct ResolvedSelect {
    table_name: String,
    projection: Vec<DatumDesc>,
    predicate: Vec<(DatumDesc, Filter)>,
    predicate_or: Vec<Vec<(DatumDesc, Filter)>>,
    placeholder: Vec<DatumDesc>,
}

impl ResolvedSelect {
    pub fn new(
        table_name: String,
        projection: Vec<DatumDesc>,
        predicate: Vec<(DatumDesc, Filter)>,
        predicate_or: Vec<Vec<(DatumDesc, Filter)>>,
        placeholder: Vec<DatumDesc>,
    ) -> Self {
        Self {
            table_name,
            projection,
            predicate,
            predicate_or,
            placeholder,
        }
    }

    pub fn table_name(&self) -> &String {
        &self.table_name
    }

    pub fn projection(&self) -> &Vec<DatumDesc> {
        &self.projection
    }

    pub fn predicate(&self) -> &Vec<(DatumDesc, Filter)> {
        &self.predicate
    }

    pub fn predicate_or(&self) -> &Vec<Vec<(DatumDesc, Filter)>> {
        &self.predicate_or
    }

    pub fn placeholder(&self) -> &Vec<DatumDesc> {
        &self.placeholder
    }
}
