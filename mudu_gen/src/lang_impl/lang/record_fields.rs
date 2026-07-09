use crate::lang_impl::lang::lang_data_type::{
    assemblyscript_default_value_expr, csharp_default_value_expr, csharp_is_reference_type,
    uni_data_type_to_name,
};
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::record_info::RecordFieldInfo;
use mudu::common::result::RS;
use mudu::utils::case_convert::{to_pascal_case, to_snake_case};
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_def::RecordField;

/// Convert a slice of [`RecordField`]s into language-normalized [`RecordFieldInfo`]s.
pub fn to_field_info(fields: &[RecordField], lang: &LangKind) -> RS<Vec<RecordFieldInfo>> {
    let mut vec = Vec::new();
    for (i, field) in fields.iter().enumerate() {
        let field_name = if *lang == LangKind::CSharp {
            to_pascal_case(&field.rf_name)
        } else {
            to_snake_case(&field.rf_name)
        };
        let field_type = uni_data_type_to_name(&field.rf_type, lang)?;
        let is_option = matches!(field.rf_type, UniDataType::Option(_));
        let (rf_required, rf_default_value, rf_deserialize_suffix) = match *lang {
            LangKind::CSharp => {
                let is_reference = csharp_is_reference_type(&field.rf_type);
                let required = is_reference && !is_option;
                (
                    required,
                    if is_option {
                        "default".to_string()
                    } else {
                        csharp_default_value_expr(&field.rf_type)?
                    },
                    if required {
                        "!".to_string()
                    } else {
                        String::new()
                    },
                )
            }
            LangKind::AssemblyScript => (
                false,
                assemblyscript_default_value_expr(&field.rf_type)?,
                String::new(),
            ),
            LangKind::Rust => (false, String::new(), String::new()),
        };
        let field_ru = RecordFieldInfo {
            rf_index: i as _,
            rf_comments: field.rf_comments.clone(),
            rf_name: field_name,
            rf_type: field_type,
            rf_required,
            rf_default_value,
            rf_deserialize_suffix,
        };
        vec.push(field_ru);
    }
    Ok(vec)
}
