use crate::universal::uni_dat_type_id::UniDatTypeId;
use mudu::common::into_result::ToResult;
use mudu::common::result::RS;
use mudu::common::result_from::ResultFrom;
use mudu_type::dat_type_id::DatTypeID;

impl UniDatTypeId {
    pub fn uni_to(self) -> RS<DatTypeID> {
        let ty_id = match self {
            Self::I32 => DatTypeID::I32,
            Self::I64 => DatTypeID::I64,
            Self::F32 => DatTypeID::F32,
            Self::F64 => DatTypeID::F64,
            Self::String => DatTypeID::String,
            Self::Array => DatTypeID::Array,
            Self::Record => DatTypeID::Record,
            _ => {
                unimplemented!()
            }
        };
        Ok(ty_id)
    }

    pub fn uni_from(ty: DatTypeID) -> RS<Self> {
        let uni_ty = match ty {
            DatTypeID::I32 => Self::I32,
            DatTypeID::I64 => Self::I64,
            DatTypeID::F32 => Self::F32,
            DatTypeID::F64 => Self::F64,
            DatTypeID::String => Self::String,
            DatTypeID::Array => Self::Array,
            DatTypeID::Record => Self::Record,
            DatTypeID::Binary => Self::Binary,
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
