use crate::universal::uni_dat_type_id::UniDatTypeId;
use mudu::common::into_result::ToResult;
use mudu::common::result::RS;
use mudu::common::result_from::ResultFrom;
use mudu::error::ec::EC;
use mudu::m_error;
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
            _ => return Err(m_error!(EC::TypeErr, "unsupported universal data type id")),
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
