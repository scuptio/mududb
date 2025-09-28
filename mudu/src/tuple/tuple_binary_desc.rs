use crate::data_type::len_kind::LenKind;
use crate::data_type::type_desc::TypeDesc;
use crate::tuple::field_desc::FieldDesc;
use crate::tuple::slot::Slot;
use serde::{Deserialize, Serialize};


#[derive(
    Clone, Debug,
    Serialize,
    Deserialize
)]
pub struct TupleBinaryDesc {
    offset_len_data_fixed: Vec<FieldDesc>,
    offset_len_slot_var: Vec<FieldDesc>,
    slot_all: Vec<FieldDesc>,
    fixed_count: usize,
    var_count: usize,
    total_fixed_size: usize,
    type_desc: Vec<TypeDesc>,
}

impl TupleBinaryDesc {
    pub fn from(type_desc: Vec<TypeDesc>) -> Self {
        if !is_normalized(&type_desc) {
            panic!("must be normalized");
        }
        let mut total_fixed_size: usize = 0;
        let mut fixed_count: usize = 0;
        let mut var_count: usize = 0;
        for td in type_desc.iter() {
            match td.fv_len_kind() {
                LenKind::FixedLen => {
                    total_fixed_size += td.fixed_len();
                    fixed_count += 1;
                }
                LenKind::VarLen => {
                    var_count += 1;
                }
            }
        }
        let offset_hdr = 0;
        let offset_slot_begin = offset_hdr;
        let mut offset_slot_var =
            (offset_slot_begin + total_fixed_size + var_count * Slot::size_of()) as u32;
        let mut offset_data_fixed = offset_slot_begin as u32;
        let mut offset_len_data_fixed: Vec<FieldDesc> = vec![];
        let mut offset_len_slot_var: Vec<FieldDesc> = vec![];
        let mut slot_all: Vec<FieldDesc> = vec![];
        for td in type_desc.iter() {
            match td.fv_len_kind() {
                LenKind::FixedLen => {
                    let data_len = td.fixed_len() as u32;
                    let slot = Slot::new(offset_data_fixed, data_len as _);
                    let param = td.type_param().to_object();

                    slot_all.push(FieldDesc::new(
                        slot.clone(),
                        td.data_type_id(),
                        param.clone(),
                        true,
                    ));
                    offset_len_data_fixed.push(FieldDesc::new(
                        slot,
                        td.data_type_id(),
                        param.clone(),
                        true,
                    ));
                    offset_data_fixed += data_len;
                }
                LenKind::VarLen => {
                    offset_slot_var += Slot::size_of() as u32;
                    let slot = Slot::new(offset_slot_var, Slot::size_of() as u32);
                    let param = td.type_param().to_object();
                    slot_all.push(FieldDesc::new(
                        slot.clone(),
                        td.data_type_id(),
                        param.clone(),
                        false,
                    ));
                    offset_len_slot_var.push(FieldDesc::new(
                        slot,
                        td.data_type_id(),
                        param.clone(),
                        false,
                    ));
                }
            }
        }
        Self {
            offset_len_data_fixed,
            offset_len_slot_var,
            slot_all,
            fixed_count,
            var_count,
            total_fixed_size,
            type_desc,
        }
    }

    pub fn normalized_type_desc_vec<T>(vec: Vec<(TypeDesc, T)>) -> (Vec<TypeDesc>, Vec<T>) {
        _normalized(vec)
    }

    pub fn fixed_len_field_desc(&self) -> &Vec<FieldDesc> {
        &self.offset_len_data_fixed
    }

    pub fn var_len_field_desc(&self) -> &Vec<FieldDesc> {
        &self.offset_len_slot_var
    }

    pub fn field_desc(&self) -> &Vec<FieldDesc> {
        &self.slot_all
    }
    pub fn field_count(&self) -> usize {
        self.type_desc.len()
    }

    pub fn fixed_field_count(&self) -> usize {
        self.fixed_count
    }

    pub fn get_field_desc(&self, idx: usize) -> &FieldDesc {
        &self.slot_all[idx]
    }

    pub fn total_slot_size(&self) -> usize {
        self.var_count * Slot::size_of()
    }

    pub fn meta_size(&self) -> usize {
        self.total_slot_size()
    }
    pub fn total_fixed_data_size(&self) -> usize {
        self.total_fixed_size
    }

    pub fn min_tuple_size(&self) -> usize {
        self.meta_size() + self.total_fixed_data_size()
    }
}

/// return the vector after normalized and the payload T of the element in the original vector
fn _normalized<T>(vec_type_desc: Vec<(TypeDesc, T)>) -> (Vec<TypeDesc>, Vec<T>) {
    let mut vec = vec_type_desc;
    vec.sort_by(|(td1, _t1), (td2, _t2)| td1.cmp(td2));
    let mut sorted_vec = vec![];
    let mut payload_vec = vec![];
    for (td, t) in vec {
        sorted_vec.push(td);
        payload_vec.push(t);
    }
    (sorted_vec, payload_vec)
}


fn is_normalized(vec_type_desc: &[TypeDesc]) -> bool {
    for i in 0..vec_type_desc.len() {
        if i + 1 < vec_type_desc.len() && vec_type_desc[i] > vec_type_desc[i + 1] {
            return false;
        }
    }
    true
}
