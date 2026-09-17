use crate::lang_impl::csharp::codec_expr_cs;
use crate::lang_impl::lang::lang_data_type::{
    assemblyscript_default_value_expr, csharp_default_value_expr, csharp_is_reference_type,
    uni_data_type_to_name,
};
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::record_info::RecordFieldInfo;
use crate::lang_impl::python::py_codec::{
    PY_RECORD_BIN, py_field_default, py_from_value, py_ident, py_to_value,
};
use mudu::common::result::RS;
use mudu::utils::case_convert::{to_pascal_case, to_snake_case};
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_def::RecordField;
use std::collections::HashMap;

/// Convert a slice of [`RecordField`]s into language-normalized [`RecordFieldInfo`]s.
pub fn to_field_info(
    fields: &[RecordField],
    lang: &LangKind,
    type_kinds: &HashMap<String, String>,
) -> RS<Vec<RecordFieldInfo>> {
    let mut vec = Vec::new();
    for (i, field) in fields.iter().enumerate() {
        let field_name = if *lang == LangKind::CSharp {
            to_pascal_case(&field.rf_name)
        } else if *lang == LangKind::Python {
            py_ident(&to_snake_case(&field.rf_name))
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
                        csharp_default_value_expr(&field.rf_type, type_kinds)?
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
            LangKind::Python => (
                false,
                py_field_default(&field.rf_type, type_kinds)?,
                String::new(),
            ),
            LangKind::C | LangKind::Go => (false, String::new(), String::new()),
        };
        // Resolver-free C# read/write statements for the field; other
        // languages leave these empty (their templates do not reference
        // them).
        let (rf_serialize, rf_deserialize) = if *lang == LangKind::CSharp {
            let mut tmp = 0;
            let value_expr = format!("value.{field_name}");
            (
                codec_expr_cs::serialize_stmts(
                    &field.rf_type,
                    &value_expr,
                    "        ",
                    &mut tmp,
                    type_kinds,
                )?,
                codec_expr_cs::deserialize_stmts(
                    &field.rf_type,
                    &value_expr,
                    rf_required,
                    "        ",
                    &mut tmp,
                )?,
            )
        } else {
            (String::new(), String::new())
        };
        // Wire-runtime conversion expressions; other languages leave these
        // empty (their templates do not reference them). Rust converts `&T`
        // places through the `mp_wire` `Value` model; Python converts typed
        // places (`x.<field>` on encode, the map value `_val` on decode)
        // through the native value model.
        let (rf_to_value, rf_from_value) = if *lang == LangKind::Rust {
            (
                crate::lang_impl::rust::value_expr_rs::rust_to_value(
                    &field.rf_type,
                    &format!("&self.{field_name}"),
                    false,
                )?,
                crate::lang_impl::rust::value_expr_rs::rust_from_value(&field.rf_type, "val")?,
            )
        } else if *lang == LangKind::Python {
            (
                py_to_value(&field.rf_type, &format!("x.{field_name}"), PY_RECORD_BIN)?,
                py_from_value(&field.rf_type, "_val")?,
            )
        } else {
            (String::new(), String::new())
        };
        let field_ru = RecordFieldInfo {
            rf_index: i as _,
            // The WIT parser assigns `rf_number` after parsing; fall back to
            // the positional number for hand-built definitions.
            rf_number: if field.rf_number == 0 {
                (i + 1) as u32
            } else {
                field.rf_number
            },
            rf_comments: field.rf_comments.clone(),
            rf_name: field_name,
            rf_type: field_type,
            rf_required,
            rf_default_value,
            rf_deserialize_suffix,
            rf_serialize,
            rf_deserialize,
            rf_to_value,
            rf_from_value,
            rf_is_option: is_option,
        };
        vec.push(field_ru);
    }
    Ok(vec)
}
