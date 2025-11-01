use crate::resolver::item_value::ItemValue;
use crate::resolver::resolved_command::ResolvedCommand;
use mudu::tuple::datum_desc::DatumDesc;

pub struct ResolvedInsert {
    insert_value: Vec<(DatumDesc, ItemValue)>,
    placeholder: Vec<DatumDesc>,
}

impl ResolvedCommand for ResolvedInsert {
    fn placeholder(&self) -> &Vec<DatumDesc> {
        &self.placeholder
    }
}

impl ResolvedInsert {
    pub fn new(
        insert_value: Vec<(DatumDesc, ItemValue)>,
        placeholder: Vec<DatumDesc>,
    ) -> ResolvedInsert {
        Self {
            insert_value,
            placeholder,
        }
    }
    pub fn insert_value(&self) -> &Vec<(DatumDesc, ItemValue)> {
        &self.insert_value
    }

    pub fn placeholder(&self) -> &Vec<DatumDesc> {
        &self.placeholder
    }
}
