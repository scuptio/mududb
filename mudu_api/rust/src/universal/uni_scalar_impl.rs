use crate::universal::uni_scalar::UniScalar;
use mudu::common::into_result::ToResult;
use mudu::common::result::RS;
use mudu::common::result_from::ResultFrom;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;

impl UniScalar {
    pub fn uni_to(self) -> RS<DatType> {
        let ty = match self {
            UniScalar::Bool => {
                return Err(m_error!(EC::TypeErr, "scalar bool is not supported"));
            }
            UniScalar::U8 => return Err(m_error!(EC::TypeErr, "scalar u8 is not supported")),
            UniScalar::I8 => return Err(m_error!(EC::TypeErr, "scalar i8 is not supported")),
            UniScalar::U16 => {
                return Err(m_error!(EC::TypeErr, "scalar u16 is not supported"));
            }
            UniScalar::I16 => {
                return Err(m_error!(EC::TypeErr, "scalar i16 is not supported"));
            }
            UniScalar::U32 => {
                return Err(m_error!(EC::TypeErr, "scalar u32 is not supported"));
            }
            UniScalar::I32 => DatType::default_for(DatTypeID::I32),
            UniScalar::U64 => {
                return Err(m_error!(EC::TypeErr, "scalar u64 is not supported"));
            }
            UniScalar::U128 => DatType::default_for(DatTypeID::U128),
            UniScalar::I64 => DatType::default_for(DatTypeID::I64),
            UniScalar::I128 => DatType::default_for(DatTypeID::I128),
            UniScalar::F32 => DatType::default_for(DatTypeID::F32),
            UniScalar::F64 => DatType::default_for(DatTypeID::F64),
            UniScalar::Char => {
                return Err(m_error!(EC::TypeErr, "scalar char is not supported"));
            }
            UniScalar::String => DatType::default_for(DatTypeID::String),
            UniScalar::Blob => DatType::default_for(DatTypeID::Binary),
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
                return Err(m_error!(EC::TypeErr, "array type is not scalar"));
            }
            DatTypeID::Record => {
                return Err(m_error!(EC::TypeErr, "record type is not scalar"));
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
