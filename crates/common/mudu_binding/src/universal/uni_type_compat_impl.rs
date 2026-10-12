use crate::universal::uni_type_compat::ParamTypeTag;
use mudu_type::data_value::DataValue;
use mudu_type::datum::DatumDyn;
use mudu_type::type_family::TypeFamily;

/// Tag a parameter value (host-side checking).
///
/// A null value reports the Binary family (a pinned quirk of the value
/// semantics), so nullness is checked first.
pub fn param_type_tag_of_value(value: &DataValue) -> ParamTypeTag {
    if value.is_null() {
        return ParamTypeTag::Null;
    }
    match value.type_family() {
        Ok(TypeFamily::I32 | TypeFamily::I64 | TypeFamily::I128 | TypeFamily::U128) => {
            ParamTypeTag::Integer
        }
        Ok(TypeFamily::F32 | TypeFamily::F64) => ParamTypeTag::Float,
        Ok(TypeFamily::Numeric) => ParamTypeTag::Numeric,
        Ok(TypeFamily::Binary) => ParamTypeTag::Blob,
        Ok(
            TypeFamily::String
            | TypeFamily::Date
            | TypeFamily::Time
            | TypeFamily::Timestamp
            | TypeFamily::TimestampTz,
        ) => ParamTypeTag::Text,
        _ => ParamTypeTag::Blob,
    }
}
