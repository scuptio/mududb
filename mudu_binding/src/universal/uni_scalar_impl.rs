use crate::universal::uni_scalar::UniScalar;
use mudu::common::into_result::ToResult;
use mudu::common::result::RS;
use mudu::common::result_from::ResultFrom;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;

impl UniScalar {
    pub fn uni_to(self) -> RS<DatType> {
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
            UniScalar::I32 => DatType::default_for(DatTypeID::I32),
            UniScalar::U64 => {
                return Err(mudu_error!(
                    ErrorCode::InvalidType,
                    "scalar u64 is not supported"
                ));
            }
            UniScalar::U128 => DatType::default_for(DatTypeID::U128),
            UniScalar::I64 => DatType::default_for(DatTypeID::I64),
            UniScalar::I128 => DatType::default_for(DatTypeID::I128),
            UniScalar::F32 => DatType::default_for(DatTypeID::F32),
            UniScalar::F64 => DatType::default_for(DatTypeID::F64),
            UniScalar::Char => {
                return Err(mudu_error!(
                    ErrorCode::InvalidType,
                    "scalar char is not supported"
                ));
            }
            UniScalar::String => DatType::default_for(DatTypeID::String),
            UniScalar::Blob => DatType::new_no_param(DatTypeID::Binary),
            UniScalar::Numeric => DatType::default_for(DatTypeID::Numeric),
            UniScalar::Date => DatType::default_for(DatTypeID::Date),
            UniScalar::Time => DatType::default_for(DatTypeID::Time),
            UniScalar::Timestamp => DatType::default_for(DatTypeID::Timestamp),
            UniScalar::TimestampTz => DatType::default_for(DatTypeID::TimestampTz),
        };
        Ok(ty)
    }

    pub fn uni_from(ty: DatType) -> RS<Self> {
        let uni_scalar = match ty.dat_type_id() {
            DatTypeID::I32 => Self::I32,
            DatTypeID::I64 => Self::I64,
            DatTypeID::I128 => Self::I128,
            DatTypeID::U128 => Self::U128,
            DatTypeID::F32 => Self::F32,
            DatTypeID::F64 => Self::F64,
            DatTypeID::String => Self::String,
            DatTypeID::Numeric => Self::Numeric,
            DatTypeID::Date => Self::Date,
            DatTypeID::Time => Self::Time,
            DatTypeID::Timestamp => Self::Timestamp,
            DatTypeID::TimestampTz => Self::TimestampTz,
            DatTypeID::Array => {
                return Err(mudu_error!(
                    ErrorCode::InvalidType,
                    "array type is not scalar"
                ));
            }
            DatTypeID::Record => {
                return Err(mudu_error!(
                    ErrorCode::InvalidType,
                    "record type is not scalar"
                ));
            }
            DatTypeID::Binary => Self::Blob,
        };
        Ok(uni_scalar)
    }
}

impl ToResult<DatType> for UniScalar {
    fn to(self) -> RS<DatType> {
        self.uni_to()
    }
}

impl ResultFrom<DatType> for UniScalar {
    fn from(value: DatType) -> RS<Self> {
        Self::uni_from(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mudu_type::dat_type_id::DatTypeID;

    #[test]
    fn supported_uni_to_mudu_roundtrip() {
        let cases = [
            (UniScalar::I32, DatTypeID::I32),
            (UniScalar::I64, DatTypeID::I64),
            (UniScalar::I128, DatTypeID::I128),
            (UniScalar::U128, DatTypeID::U128),
            (UniScalar::F32, DatTypeID::F32),
            (UniScalar::F64, DatTypeID::F64),
            (UniScalar::String, DatTypeID::String),
            (UniScalar::Blob, DatTypeID::Binary),
            (UniScalar::Numeric, DatTypeID::Numeric),
            (UniScalar::Date, DatTypeID::Date),
            (UniScalar::Time, DatTypeID::Time),
            (UniScalar::Timestamp, DatTypeID::Timestamp),
            (UniScalar::TimestampTz, DatTypeID::TimestampTz),
        ];
        for (uni, expected_id) in cases {
            let dat = uni.uni_to().unwrap();
            assert_eq!(dat.dat_type_id(), expected_id);
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
        for dat_id in [DatTypeID::Array, DatTypeID::Record] {
            let err = UniScalar::uni_from(DatType::new_no_param(dat_id)).unwrap_err();
            assert_eq!(err.ec(), ErrorCode::InvalidType);
        }
    }

    #[test]
    fn result_from_and_to_result_traits() {
        let dat: DatType = UniScalar::I64.to().unwrap();
        assert_eq!(dat.dat_type_id(), DatTypeID::I64);

        let back: UniScalar = ResultFrom::from(DatType::default_for(DatTypeID::String)).unwrap();
        assert_eq!(back, UniScalar::String);
    }
}
