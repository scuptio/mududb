use crate::universal::uni_dat_type_id::UniDatTypeId;
use mudu::common::into_result::ToResult;
use mudu::common::result::RS;
use mudu::common::result_from::ResultFrom;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_type::dat_type_id::DatTypeID;

impl UniDatTypeId {
    pub fn uni_to(self) -> RS<DatTypeID> {
        let ty_id = match self {
            Self::I32 => DatTypeID::I32,
            Self::I64 => DatTypeID::I64,
            Self::OID => DatTypeID::U128,
            Self::I128 => DatTypeID::I128,
            Self::F32 => DatTypeID::F32,
            Self::F64 => DatTypeID::F64,
            Self::String => DatTypeID::String,
            Self::Array => DatTypeID::Array,
            Self::Record => DatTypeID::Record,
            Self::Binary => DatTypeID::Binary,
            Self::Numeric => DatTypeID::Numeric,
            Self::Date => DatTypeID::Date,
            Self::Time => DatTypeID::Time,
            Self::Timestamp => DatTypeID::Timestamp,
            Self::TimestampTz => DatTypeID::TimestampTz,
            _ => {
                return Err(mudu_error!(
                    ErrorCode::InvalidType,
                    "unsupported universal data type id"
                ));
            }
        };
        Ok(ty_id)
    }

    pub fn uni_from(ty: DatTypeID) -> RS<Self> {
        let uni_ty = match ty {
            DatTypeID::I32 => Self::I32,
            DatTypeID::I64 => Self::I64,
            DatTypeID::U128 => Self::OID,
            DatTypeID::I128 => Self::I128,
            DatTypeID::F32 => Self::F32,
            DatTypeID::F64 => Self::F64,
            DatTypeID::String => Self::String,
            DatTypeID::Array => Self::Array,
            DatTypeID::Record => Self::Record,
            DatTypeID::Binary => Self::Binary,
            DatTypeID::Numeric => Self::Numeric,
            DatTypeID::Date => Self::Date,
            DatTypeID::Time => Self::Time,
            DatTypeID::Timestamp => Self::Timestamp,
            DatTypeID::TimestampTz => Self::TimestampTz,
        };
        Ok(uni_ty)
    }
}

impl ResultFrom<DatTypeID> for UniDatTypeId {
    fn from(ty_id: DatTypeID) -> RS<UniDatTypeId> {
        Self::uni_from(ty_id)
    }
}

impl ToResult<DatTypeID> for UniDatTypeId {
    fn to(self) -> RS<DatTypeID> {
        self.uni_to()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_uni_to_mudu_roundtrip() {
        let cases = [
            (UniDatTypeId::I32, DatTypeID::I32),
            (UniDatTypeId::I64, DatTypeID::I64),
            (UniDatTypeId::OID, DatTypeID::U128),
            (UniDatTypeId::I128, DatTypeID::I128),
            (UniDatTypeId::F32, DatTypeID::F32),
            (UniDatTypeId::F64, DatTypeID::F64),
            (UniDatTypeId::String, DatTypeID::String),
            (UniDatTypeId::Array, DatTypeID::Array),
            (UniDatTypeId::Record, DatTypeID::Record),
            (UniDatTypeId::Binary, DatTypeID::Binary),
            (UniDatTypeId::Numeric, DatTypeID::Numeric),
            (UniDatTypeId::Date, DatTypeID::Date),
            (UniDatTypeId::Time, DatTypeID::Time),
            (UniDatTypeId::Timestamp, DatTypeID::Timestamp),
            (UniDatTypeId::TimestampTz, DatTypeID::TimestampTz),
        ];
        for (uni, dat) in cases {
            assert_eq!(uni.uni_to().unwrap(), dat);
            assert_eq!(UniDatTypeId::uni_from(dat).unwrap(), uni);
        }
    }

    #[test]
    fn supported_uni_from_covers_all_dat_type_ids() {
        let cases = [
            DatTypeID::I32,
            DatTypeID::I64,
            DatTypeID::U128,
            DatTypeID::I128,
            DatTypeID::F32,
            DatTypeID::F64,
            DatTypeID::String,
            DatTypeID::Array,
            DatTypeID::Record,
            DatTypeID::Binary,
            DatTypeID::Numeric,
            DatTypeID::Date,
            DatTypeID::Time,
            DatTypeID::Timestamp,
            DatTypeID::TimestampTz,
        ];
        for dat in cases {
            assert!(UniDatTypeId::uni_from(dat).is_ok());
        }
    }

    #[test]
    fn unsupported_uni_to_returns_invalid_type() {
        let unsupported = [
            UniDatTypeId::Bool,
            UniDatTypeId::U8,
            UniDatTypeId::I8,
            UniDatTypeId::U16,
            UniDatTypeId::I16,
            UniDatTypeId::U32,
            UniDatTypeId::U64,
            UniDatTypeId::Char,
        ];
        for uni in unsupported {
            let err = uni.uni_to().unwrap_err();
            assert_eq!(err.ec(), ErrorCode::InvalidType);
        }
    }

    #[test]
    fn result_from_and_to_result_traits() {
        let uni = UniDatTypeId::I64;
        let dat: DatTypeID = uni.to().unwrap();
        assert_eq!(dat, DatTypeID::I64);

        let back: UniDatTypeId = ResultFrom::from(DatTypeID::I64).unwrap();
        assert_eq!(back, UniDatTypeId::I64);
    }
}
