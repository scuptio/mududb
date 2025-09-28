use crate::resolver::filter::Filter;
use crate::resolver::item_value::ItemValue;
use crate::resolver::resolved_command::ResolvedCommand;
use mudu::tuple::datum_desc::DatumDesc;

pub struct ResolvedUpdate {
    table_name: String,
    set_value: Vec<(DatumDesc, ItemValue)>,
    predicate: Vec<(DatumDesc, Filter)>,
    predicate_or: Vec<Vec<(DatumDesc, Filter)>>,
    place_holder: Vec<DatumDesc>,
}


impl ResolvedUpdate {
    pub fn new(
        table_name: String,
        set_value: Vec<(DatumDesc, ItemValue)>,
        predicate: Vec<(DatumDesc, Filter)>,
        predicate_or: Vec<Vec<(DatumDesc, Filter)>>,
        place_holder: Vec<DatumDesc>,
    ) -> Self {
        Self {
            table_name,
            set_value,
            predicate,
            predicate_or,
            place_holder,
        }
    }

    pub fn table_name(&self) -> &String {
        &self.table_name
    }

    pub fn predicate(&self) -> &Vec<(DatumDesc, Filter)> {
        &self.predicate
    }

    pub fn predicate_or(&self) -> &Vec<Vec<(DatumDesc, Filter)>> {
        &self.predicate_or
    }

    pub fn set_value(&self) -> &Vec<(DatumDesc, ItemValue)> {
        &self.set_value
    }
}


impl ResolvedCommand for ResolvedUpdate {
    fn placeholder(&self) -> &Vec<DatumDesc> {
        &self.place_holder
    }
}