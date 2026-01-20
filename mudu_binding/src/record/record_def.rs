use crate::record::field_def::FieldDef;
use crate::universal::uni_dat_type::UniDatType;
use crate::universal::uni_record_type::{UniRecordField, UniRecordType};
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct RecordDef {
    record_name: String,
    fields: Vec<FieldDef>,
    name2fields: HashMap<String, FieldDef>,
}

impl RecordDef {
    pub fn update_field_inline(&mut self, ty: &UniDatType) -> RS<()> {
        let record = if let UniDatType::Record(record) = ty {
            record
        } else {
            return Err(m_error!(EC::FatalError, "expected a record type"));
        };
        if record.record_name != self.record_name {
            return Err(m_error!(EC::FatalError, "expected name equal"));
        }
        if record.record_fields.len() != self.fields.len() {
            return Err(m_error!(EC::FatalError, "expected table columns equal"));
        }
        for (i, column) in self.fields.iter_mut().enumerate() {
            if column.column_name() != &record.record_fields[i].field_name {
                return Err(m_error!(EC::FatalError, "expected column name equal"));
            }
            column.set_column_type(record.record_fields[i].field_type.clone());
        }
        Ok(())
    }
    pub fn to_record_type(&self) -> RS<UniRecordType> {
        let mut record_fields = Vec::with_capacity(self.fields.len());
        for column in self.fields.iter() {
            let field_type = column.dat_type().clone();
            let field_name = column.column_name().clone();
            let record_field = UniRecordField {
                field_name,
                field_type,
            };
            record_fields.push(record_field)
        }
        Ok(UniRecordType {
            record_name: self.record_name.clone(),
            record_fields,
        })
    }

    pub fn new(table_name: String, table_columns: Vec<FieldDef>) -> Self {
        let mut name2column_def = HashMap::new();
        for c in table_columns.iter() {
            name2column_def.insert(c.column_name().clone(), c.clone());
        }
        Self {
            record_name: table_name,
            fields: table_columns,
            name2fields: name2column_def,
        }
    }

    pub fn table_name(&self) -> &String {
        &self.record_name
    }

    pub fn table_columns(&self) -> &Vec<FieldDef> {
        &self.fields
    }

    pub fn row_desc(&self) -> RS<TupleFieldDesc> {
        let mut vec = vec![];
        for c in &self.fields {
            let dd = DatumDesc::new(c.column_name().clone(), c.dat_type().clone().uni_to()?);
            vec.push(dd);
        }
        Ok(TupleFieldDesc::new(vec))
    }

    pub fn find_column_def_by_name(&self, name: &str) -> Option<&FieldDef> {
        self.name2fields.get(name)
    }
}
