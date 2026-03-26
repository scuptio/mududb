use crate::lang_impl::lang::lang_data_type::{
    csharp_default_value_expr, csharp_is_reference_type, uni_data_type_to_name,
};
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::record_info::RecordFieldInfo;
use mudu::common::result::RS;
use mudu::utils::case_convert::{to_pascal_case, to_snake_case};
use mudu_binding::universal::uni_def::RecordField;

pub fn to_field_info(fields: &Vec<RecordField>, lang: &LangKind) -> RS<Vec<RecordFieldInfo>> {
    let mut vec = Vec::new();
    for (i, field) in fields.iter().enumerate() {
        let field_name = if *lang == LangKind::CSharp {
            to_pascal_case(&field.rf_name)
        } else {
            to_snake_case(&field.rf_name)
        };
        let field_type = uni_data_type_to_name(&field.rf_type, lang)?;
        let (rf_required, rf_default_value) = if *lang == LangKind::CSharp {
            (
                csharp_is_reference_type(&field.rf_type),
                csharp_default_value_expr(&field.rf_type)?,
            )
        } else {
            (false, String::new())
        };
        let field_ru = RecordFieldInfo {
            rf_index: i as _,
            rf_comments: field.rf_comments.clone(),
            rf_name: field_name,
            rf_type: field_type,
            rf_required,
            rf_default_value,
        };
        vec.push(field_ru);
    }
    Ok(vec)
}
