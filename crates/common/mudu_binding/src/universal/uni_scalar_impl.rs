use crate::universal::uni_scalar::UniScalar;
use mudu::common::into_result::ToResult;
use mudu::common::result::RS;
use mudu::common::result_from::ResultFrom;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_type::data_type::DataType;
use mudu_type::type_family::TypeFamily;

impl UniScalar {
    pub fn uni_to(self) -> RS<DataType> {
        let ty = match self {
            UniScalar::Bool => {
                return Err(mudu_error!(
                    ErrorCode::InvalidType,
                    "scalar bool is not supported"
                ));
            }
            UniScalar::U8 => {
                return Err(mudu_error!(
                    ErrorCode::InvalidType,
                    "scalar u8 is not supported"
                ));
            }
            UniScalar::I8 => {
                return Err(mudu_error!(
                    ErrorCode::InvalidType,
                    "scalar i8 is not supported"
                ));
            }
            UniScalar::U16 => {
                return Err(mudu_error!(
                    ErrorCode::InvalidType,
                    "scalar u16 is not supported"
                ));
            }
            UniScalar::I16 => {
                return Err(mudu_error!(
                    ErrorCode::InvalidType,
                    "scalar i16 is not supported"
                ));
            }
            UniScalar::U32 => {
                return Err(mudu_error!(
                    ErrorCode::InvalidType,
                    "scalar u32 is not supported"
                ));
            }
            UniScalar::I32 => DataType::default_for(TypeFamily::I32),
            UniScalar::U64 => {
                return Err(mudu_error!(
                    ErrorCode::InvalidType,
                    "scalar u64 is not supported"
                ));
            }
            UniScalar::U128 => DataType::default_for(TypeFamily::U128),
            UniScalar::I64 => DataType::default_for(TypeFamily::I64),
            UniScalar::I128 => DataType::default_for(TypeFamily::I128),
            UniScalar::F32 => DataType::default_for(TypeFamily::F32),
            UniScalar::F64 => DataType::default_for(TypeFamily::F64),
            UniScalar::Char => {
                return Err(mudu_error!(
                    ErrorCode::InvalidType,
                    "scalar char is not supported"
                ));
            }
            UniScalar::String => DataType::default_for(TypeFamily::String),
            UniScalar::Blob => DataType::new_no_param(TypeFamily::Binary),
            UniScalar::Numeric => DataType::default_for(TypeFamily::Numeric),
            UniScalar::Date => DataType::default_for(TypeFamily::Date),
            UniScalar::Time => DataType::default_for(TypeFamily::Time),
            UniScalar::Timestamp => DataType::default_for(TypeFamily::Timestamp),
            UniScalar::TimestampTz => DataType::default_for(TypeFamily::TimestampTz),
        };
        Ok(ty)
    }

    pub fn uni_from(ty: DataType) -> RS<Self> {
        let uni_scalar = match ty.type_family() {
            TypeFamily::I32 => Self::I32,
            TypeFamily::I64 => Self::I64,
            TypeFamily::I128 => Self::I128,
            TypeFamily::U128 => Self::U128,
            TypeFamily::F32 => Self::F32,
            TypeFamily::F64 => Self::F64,
            TypeFamily::String => Self::String,
            TypeFamily::Numeric => Self::Numeric,
            TypeFamily::Date => Self::Date,
            TypeFamily::Time => Self::Time,
            TypeFamily::Timestamp => Self::Timestamp,
            TypeFamily::TimestampTz => Self::TimestampTz,
            TypeFamily::Array => {
                return Err(mudu_error!(
                    ErrorCode::InvalidType,
                    "array type is not scalar"
                ));
            }
            TypeFamily::Record => {
                return Err(mudu_error!(
                    ErrorCode::InvalidType,
                    "record type is not scalar"
                ));
            }
            TypeFamily::Binary => Self::Blob,
        };
        Ok(uni_scalar)
    }
}

impl ToResult<DataType> for UniScalar {
    fn to(self) -> RS<DataType> {
        self.uni_to()
    }
}

impl ResultFrom<DataType> for UniScalar {
    fn from(value: DataType) -> RS<Self> {
        Self::uni_from(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mudu_type::type_family::TypeFamily;

    #[test]
    fn supported_uni_to_mudu_roundtrip() {
        let cases = [
            (UniScalar::I32, TypeFamily::I32),
            (UniScalar::I64, TypeFamily::I64),
            (UniScalar::I128, TypeFamily::I128),
            (UniScalar::U128, TypeFamily::U128),
            (UniScalar::F32, TypeFamily::F32),
            (UniScalar::F64, TypeFamily::F64),
            (UniScalar::String, TypeFamily::String),
            (UniScalar::Blob, TypeFamily::Binary),
            (UniScalar::Numeric, TypeFamily::Numeric),
            (UniScalar::Date, TypeFamily::Date),
            (UniScalar::Time, TypeFamily::Time),
            (UniScalar::Timestamp, TypeFamily::Timestamp),
            (UniScalar::TimestampTz, TypeFamily::TimestampTz),
        ];
        for (uni, expected_id) in cases {
            let dat = uni.uni_to().unwrap();
            assert_eq!(dat.type_family(), expected_id);
            assert_eq!(UniScalar::uni_from(dat).unwrap(), uni);
        }
    }

    #[test]
    fn unsupported_uni_to_returns_invalid_type() {
        let unsupported = [
            UniScalar::Bool,
            UniScalar::U8,
            UniScalar::I8,
            UniScalar::U16,
            UniScalar::I16,
            UniScalar::U32,
            UniScalar::U64,
            UniScalar::Char,
        ];
        for uni in unsupported {
            let err = uni.uni_to().unwrap_err();
            assert_eq!(err.ec(), ErrorCode::InvalidType);
        }
    }

    #[test]
    fn non_scalar_uni_from_rejected() {
        for dat_id in [TypeFamily::Array, TypeFamily::Record] {
            let err = UniScalar::uni_from(DataType::new_no_param(dat_id)).unwrap_err();
            assert_eq!(err.ec(), ErrorCode::InvalidType);
        }
    }

    #[test]
    fn result_from_and_to_result_traits() {
        let dat: DataType = UniScalar::I64.to().unwrap();
        assert_eq!(dat.type_family(), TypeFamily::I64);

        let back: UniScalar = ResultFrom::from(DataType::default_for(TypeFamily::String)).unwrap();
        assert_eq!(back, UniScalar::String);
    }
}
