use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_scalar::UniScalar;

use super::c_type_name;
use super::holder::CTypeNamer;

/// The C type of a value in codec position.
///
/// Composites are named through `namer`: `list<T>` maps to a generated slice
/// struct, `option<T>` to a generated wrapper struct, `box<T>` to `T *` and
/// tuples to generated `f0..fN` structs.
pub fn c_type(ty: &UniDataType, namer: &mut dyn CTypeNamer) -> RS<String> {
    let s = match ty {
        UniDataType::Scalar(scalar) => match scalar {
            UniScalar::Bool => "bool".to_string(),
            UniScalar::U8 => "uint8_t".to_string(),
            UniScalar::U16 => "uint16_t".to_string(),
            UniScalar::U32 => "uint32_t".to_string(),
            UniScalar::U64 => "uint64_t".to_string(),
            UniScalar::I8 => "int8_t".to_string(),
            UniScalar::I16 => "int16_t".to_string(),
            UniScalar::I32 => "int32_t".to_string(),
            UniScalar::I64 => "int64_t".to_string(),
            UniScalar::F32 => "float".to_string(),
            UniScalar::F64 => "double".to_string(),
            UniScalar::Char => "char".to_string(),
            UniScalar::String
            | UniScalar::Numeric
            | UniScalar::Date
            | UniScalar::Time
            | UniScalar::Timestamp
            | UniScalar::TimestampTz => "mp_str".to_string(),
            UniScalar::Blob => "mp_bin".to_string(),
            UniScalar::U128 | UniScalar::I128 => {
                return Err(mudu_error!(
                    ErrorCode::NotImplemented,
                    "C codec does not support 128-bit integers (the uni surface carries them as list<u8>)"
                ));
            }
        },
        UniDataType::Binary => "mp_bin".to_string(),
        UniDataType::Array(inner) => namer.list_name(inner)?,
        UniDataType::Option(inner) => namer.option_name(inner)?,
        UniDataType::Tuple(elems) => namer.tuple_name(elems)?,
        UniDataType::Box(inner) => format!("{} *", c_type(inner, namer)?),
        UniDataType::Identifier(name) => c_type_name(name),
        UniDataType::Result(_) | UniDataType::Record { .. } => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!("C codec does not support {:?} in this position", ty)
            ));
        }
    };
    Ok(s)
}
