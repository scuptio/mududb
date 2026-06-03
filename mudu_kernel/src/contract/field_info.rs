use mudu::common::id::{AttrIndex, DatumIndex, OID};
use mudu_type::dt_fn_param::DatType;

#[derive(Clone, Debug, Default)]
pub struct FieldInfo {
    name: String,
    id: OID,
    type_desc: DatType,
    // index in key or value tuple
    datum_index: DatumIndex,
    // index in original create table column definition list
    column_index: AttrIndex,
    primary_index: Option<AttrIndex>,
    nullable: bool,
}

impl FieldInfo {
    pub fn new(
        name: String,
        id: OID,
        type_desc: DatType,
        datum_index: DatumIndex,
        column_index: AttrIndex,
        primary_index: Option<AttrIndex>,
        nullable: bool,
    ) -> Self {
        Self {
            name,
            id,
            type_desc,
            datum_index,
            column_index,
            primary_index,
            nullable,
        }
    }

    pub fn name(&self) -> &String {
        &self.name
    }

    pub fn id(&self) -> OID {
        self.id
    }

    pub fn column_index(&self) -> AttrIndex {
        self.column_index
    }

    pub fn is_primary(&self) -> bool {
        self.primary_index.is_some()
    }

    pub fn primary_index(&self) -> Option<AttrIndex> {
        self.primary_index
    }

    pub fn datum_index(&self) -> DatumIndex {
        self.datum_index
    }

    pub fn set_datum_index(&mut self, index: DatumIndex) {
        self.datum_index = index;
    }

    pub fn type_desc(&self) -> &DatType {
        &self.type_desc
    }

    pub fn nullable(&self) -> bool {
        self.nullable
    }
}
