use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::record_fields::to_field_info;
use mudu::common::result::RS;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::universal::uni_def::UniRecordDef;

/// Language-normalized record metadata.
#[derive(Debug, Clone)]
pub struct RecordInfo {
    /// Record doc comments.
    pub record_comments: String,
    /// Pascal-case record name.
    pub record_name: String,
    /// Normalized record fields.
    pub record_fields: Vec<RecordFieldInfo>,
}

/// Language-normalized record field metadata.
#[derive(Debug, Clone)]
pub struct RecordFieldInfo {
    /// Field index.
    #[allow(dead_code)]
    pub rf_index: u32,
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
}

impl RecordInfo {
    /// Convert a [`UniRecordDef`] into a [`RecordInfo`] for the target language.
    pub fn from(record_def: UniRecordDef, lang: LangKind) -> RS<Self> {
        let name = to_pascal_case(&record_def.record_name);
        let record_fields = to_field_info(&record_def.record_fields, &lang)?;
        let record_info = RecordInfo {
            record_comments: record_def.record_comments,
            record_name: name,
            record_fields,
        };
        Ok(record_info)
    }
}

#[cfg(test)]
mod tests {
    use super::RecordInfo;
    use crate::lang_impl::lang::lang_kind::LangKind;
    use mudu::common::result::RS;
    use mudu_binding::universal::uni_data_type::UniDataType;
    use mudu_binding::universal::uni_def::{RecordField, UniRecordDef};
    use mudu_binding::universal::uni_scalar::UniScalar;

    #[test]
    fn from_normalizes_record_metadata() -> RS<()> {
        let record_def = UniRecordDef {
            record_comments: "comment".to_string(),
            record_name: "mu-oid".to_string(),
            record_fields: vec![
                RecordField {
                    rf_comments: "high bits".to_string(),
                    rf_name: "h".to_string(),
                    rf_type: UniDataType::Scalar(UniScalar::U64),
                },
                RecordField {
                    rf_comments: "low bits".to_string(),
                    rf_name: "l".to_string(),
                    rf_type: UniDataType::Scalar(UniScalar::U64),
                },
            ],
        };
        let info = RecordInfo::from(record_def, LangKind::Rust)?;
        assert_eq!(info.record_name, "MuOid");
        assert_eq!(info.record_fields.len(), 2);
        assert_eq!(info.record_fields[0].rf_name, "h");
        assert_eq!(info.record_fields[1].rf_type, "u64");
        Ok(())
    }

    #[test]
    fn from_uses_pascal_case_for_csharp() -> RS<()> {
        let record_def = UniRecordDef {
            record_comments: String::new(),
            record_name: "mu-error".to_string(),
            record_fields: vec![RecordField {
                rf_comments: String::new(),
                rf_name: "err-msg".to_string(),
                rf_type: UniDataType::Scalar(UniScalar::String),
            }],
        };
        let info = RecordInfo::from(record_def, LangKind::CSharp)?;
        assert_eq!(info.record_fields[0].rf_name, "ErrMsg");
        assert_eq!(info.record_fields[0].rf_type, "string");
        assert!(info.record_fields[0].rf_required);
        assert_eq!(info.record_fields[0].rf_default_value, "string.Empty");
        Ok(())
    }
}
