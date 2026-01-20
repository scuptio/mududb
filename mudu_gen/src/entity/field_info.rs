use crate::lang_impl::lang::lang_data_type::uni_data_type_to_name;
use crate::lang_impl::lang::lang_kind::LangKind;
use mudu::common::result::RS;
use mudu::utils::case_convert::{to_pascal_case, to_snake_case, to_snake_case_upper};
use mudu_binding::record::field_def::FieldDef;


#[derive(Debug)]
pub struct FieldInfo {
    pub field_name: String,
    pub field_name_snake_case: String,
    pub field_name_pascal_case : String,
    pub data_type: String, // Ex. i32, String
    pub attr_struct_name: String,
    pub field_name_const: String,
}


impl FieldInfo {
    pub fn from_column_schema(
        table_name: &String,
        column_schema: &FieldDef,
        lang: &LangKind
    ) -> RS<Self> {
        Ok(Self {
            field_name: column_schema.column_name().clone(),
            field_name_snake_case: to_snake_case(column_schema.column_name()),
            field_name_pascal_case: to_pascal_case(column_schema.column_name()),
            data_type: uni_data_type_to_name(column_schema.dat_type(), lang)?,
            attr_struct_name: to_pascal_case(table_name),
            field_name_const: to_snake_case_upper(column_schema.column_name()),
        })
    }
}