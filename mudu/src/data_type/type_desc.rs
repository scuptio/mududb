use crate::data_type::dt_impl::dat_type_id::DatTypeID;
use crate::data_type::len_kind::LenKind;
use crate::data_type::param_info::ParamInfo;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

#[derive(Eq, PartialEq, Clone, Debug, Hash, Serialize, Deserialize)]
pub struct TypeDesc {
    type_id: DatTypeID,
    max_len: Option<usize>,
    fv_len_kind: LenKind,
    type_param: ParamInfo,
}

impl TypeDesc {
    pub fn new(dt_id: DatTypeID, type_param: ParamInfo) -> Self {
        let max_len = dt_id.type_len(&type_param.to_object());
        Self {
            type_id: dt_id,
            max_len,
            fv_len_kind: LenKind::new(dt_id.is_fixed_len()),
            type_param,
        }
    }

    pub fn data_type_id(&self) -> DatTypeID {
        self.type_id
    }

    pub fn type_param(&self) -> &ParamInfo {
        &self.type_param
    }

    pub fn is_fixed_len(&self) -> bool {
        match self.fv_len_kind {
            LenKind::FixedLen => true,
            LenKind::VarLen => false,
        }
    }

    pub fn fv_len_kind(&self) -> LenKind {
        self.fv_len_kind
    }
    pub fn fixed_len(&self) -> usize {
        match (self.fv_len_kind, self.max_len) {
            (LenKind::FixedLen, Some(len)) => len,
            (_, _) => {
                unreachable!()
            }
        }
    }

    /// the max length for a fixed length type is the data length
    pub fn opt_max_len(&self) -> Option<usize> {
        self.max_len
    }
}

impl PartialOrd<Self> for TypeDesc {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// the order is used to organize value the tuple_slice layout
// the fewer length data type comes first in tuple_slice layout
impl Ord for TypeDesc {
    fn cmp(&self, other: &Self) -> Ordering {
        match (&self.fv_len_kind, &other.fv_len_kind) {
            (LenKind::FixedLen, LenKind::FixedLen) => {
                self.type_id.to_u32().cmp(&other.type_id.to_u32())
            }
            (LenKind::VarLen, LenKind::VarLen) => {
                self.type_id.to_u32().cmp(&other.type_id.to_u32())
            }
            (LenKind::FixedLen, LenKind::VarLen) => Ordering::Less,
            (LenKind::VarLen, LenKind::FixedLen) => Ordering::Greater,
        }
    }
}
