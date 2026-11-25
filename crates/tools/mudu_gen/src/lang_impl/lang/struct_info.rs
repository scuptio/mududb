/// Language-normalized record metadata.
#[derive(Debug, Clone)]
pub struct StructInfo {
    /// Struct doc comments.
    pub struct_comments: String,
    /// Pascal-case record name.
    pub struct_name: String,
    /// Normalized record fields.
    pub struct_fields: Vec<StructFieldInfo>,
}

/// Language-normalized record field metadata.
#[derive(Debug, Clone)]
pub struct StructFieldInfo {
    /// 1-based field number in WIT declaration order (the wire map key).
    pub rf_number: u32,
    /// Field doc comments.
    pub rf_comments: String,
    /// Field name.
    pub rf_name: String,
    /// Language-specific field type.
    pub rf_type: String,
    /// Whether the field is required (C# reference-type tracking).
    pub rf_required: bool,
    /// Default-value expression for the field.
    pub rf_default_value: String,
    /// Suffix used when deserializing the field (C# null-forgiving operator).
    pub rf_deserialize_suffix: String,
    /// C# resolver-free serialize statements writing the field to `writer`
    /// (empty for other languages).
    pub rf_serialize: String,
    /// C# resolver-free deserialize statements reading the field from
    /// `reader` (empty for other languages).
    pub rf_deserialize: String,
    /// Wire expression converting the typed field place to a wire value
    /// (Rust: `&self.<field>` to `Value`; Python: `x.<field>` to a native
    /// value; empty for other languages).
    pub rf_to_value: String,
    /// Wire expression converting a wire value into the typed field value
    /// (Rust: `&Value` named `val` to `Result<FieldType, WireError>`;
    /// Python: native value `_val` to the field value; empty for other
    /// languages).
    pub rf_from_value: String,
    /// Whether the field is a WIT `option<T>`; such fields are omitted from
    /// the encoded map when they hold no value (proto3 presence semantics).
    pub rf_is_option: bool,
}

#[cfg(test)]
mod tests {
    use super::StructInfo;
    use crate::lang_impl::lang::lang_kind::LangKind;
    use mudu::common::result::RS;
    use mudu_binding::universal::uni_data_type::UniDataType;
    use mudu_binding::universal::uni_def::{StructField, UniStructDef};
    use mudu_binding::universal::uni_scalar::UniScalar;

    #[test]
    fn from_normalizes_record_metadata() -> RS<()> {
        let record_def = UniStructDef {
            struct_comments: "comment".to_string(),
            struct_name: "mu-oid".to_string(),
            struct_fields: vec![
                StructField::new(
                    "high bits".to_string(),
                    "h".to_string(),
                    UniDataType::Scalar(UniScalar::U64),
                ),
                StructField::new(
                    "low bits".to_string(),
                    "l".to_string(),
                    UniDataType::Scalar(UniScalar::U64),
                ),
            ],
        };
        let info = StructInfo::from(record_def, LangKind::Rust, &Default::default())?;
        assert_eq!(info.struct_name, "MuOid");
        assert_eq!(info.struct_fields.len(), 2);
        assert_eq!(info.struct_fields[0].rf_name, "h");
        assert_eq!(info.struct_fields[1].rf_type, "u64");
        Ok(())
    }

    #[test]
    fn from_uses_pascal_case_for_csharp() -> RS<()> {
        let record_def = UniStructDef {
            struct_comments: String::new(),
            struct_name: "mu-error".to_string(),
            struct_fields: vec![StructField::new(
                String::new(),
                "err-msg".to_string(),
                UniDataType::Scalar(UniScalar::String),
            )],
        };
        let info = StructInfo::from(record_def, LangKind::CSharp, &Default::default())?;
        assert_eq!(info.struct_fields[0].rf_name, "ErrMsg");
        assert_eq!(info.struct_fields[0].rf_type, "string");
        assert!(info.struct_fields[0].rf_required);
        assert_eq!(info.struct_fields[0].rf_default_value, "string.Empty");
        Ok(())
    }
}
