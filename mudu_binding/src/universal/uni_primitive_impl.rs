use crate::universal::uni_primitive::UniPrimitive;
use mudu::common::into_result::ToResult;
use mudu::common::result::RS;
use mudu::common::result_from::ResultFrom;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;

impl UniPrimitive {
    pub fn uni_to(self) -> RS<DatType> {
        let ty = match self {
            UniPrimitive::Bool => {
                unimplemented!()
            }
            UniPrimitive::U8 => {
                unimplemented!()
            }
            UniPrimitive::I8 => {
                unimplemented!()
            }
            UniPrimitive::U16 => {
                unimplemented!()
            }
            UniPrimitive::I16 => {
                unimplemented!()
            }
            UniPrimitive::U32 => {
                unimplemented!()
            }
            UniPrimitive::I32 => DatType::default_for(DatTypeID::I32),
            UniPrimitive::U64 => {
                unimplemented!()
            }
            UniPrimitive::I64 => DatType::default_for(DatTypeID::I64),
            UniPrimitive::F32 => DatType::default_for(DatTypeID::F32),
            UniPrimitive::F64 => DatType::default_for(DatTypeID::F64),
            UniPrimitive::Char => {
                unimplemented!()
            }
            UniPrimitive::String => DatType::default_for(DatTypeID::String),
        };
        Ok(ty)
    }

    pub fn uni_from(ty: DatType) -> RS<Self> {
        let uni_prim = match ty.dat_type_id() {
            DatTypeID::I32 => Self::I32,
            DatTypeID::I64 => Self::I64,
            DatTypeID::F32 => Self::F32,
            DatTypeID::F64 => Self::F64,
            DatTypeID::String => Self::String,
            DatTypeID::Array => {
                return Err(m_error!(EC::TypeErr, "array type is not primitive"));
            }
            DatTypeID::Record => {
                return Err(m_error!(EC::TypeErr, "record type is not primitive"));
            }
            DatTypeID::Binary => {
                return Err(m_error!(EC::TypeErr, "binary type is not primitive"));
            }
        };
        Ok(uni_prim)
    }
}

impl ToResult<DatType> for UniPrimitive {
    fn to(self) -> RS<DatType> {
        self.uni_to()
    }
}

impl ResultFrom<DatType> for UniPrimitive {
    fn from(value: DatType) -> RS<Self> {
        Self::uni_from(value)
    }
}
