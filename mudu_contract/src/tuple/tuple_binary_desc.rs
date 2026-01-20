use crate::tuple::field_desc::FieldDesc;
use crate::tuple::slot::Slot;
use mudu::common::cmp_order::Order;
use mudu::common::result::RS;
use mudu::error::err::MError;
use mudu_type::dat_type::DatType;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::mem;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TupleBinaryDesc {
    offset_len_data_fixed: Vec<FieldDesc>,
    offset_len_slot_var: Vec<FieldDesc>,
    slot_all: Vec<FieldDesc>,
    fixed_count: usize,
    var_count: usize,
    total_fixed_size: usize,
    type_desc: Vec<DatType>,
}

impl TupleBinaryDesc {
    pub fn from(type_desc: Vec<DatType>) -> RS<Self> {
        if !is_normalized(&type_desc)? {
            panic!("must be normalized");
        }
        let mut total_fixed_size: usize = 0;
        let mut fixed_count: usize = 0;
        let mut var_count: usize = 0;
        for td in type_desc.iter() {
            let id = td.dat_type_id();
            match id.fn_send_type_len()(td) {
                Ok(opt_len) => {
                    match opt_len {
                        Some(len) => {
                            total_fixed_size += len as usize;
                            fixed_count += 1;
                        }
                        None => {
                            var_count += 1;
                        }
                    }
                }
                Err(e) => {
                    panic!("get type length error, {:?}", e);
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
        for ty in type_desc.iter() {
            let id = ty.dat_type_id();
            match id.fn_send_type_len()(ty) {
                Ok(opt_len) => {
                    match opt_len {
                        Some(data_len) => {
                            let slot = Slot::new(offset_data_fixed, data_len as _);
                            slot_all.push(FieldDesc::new(
                                slot.clone(),
                                ty.clone(),
                                true,
                            ));
                            offset_len_data_fixed.push(FieldDesc::new(
                                slot,
                                ty.clone(),
                                true,
                            ));
                            offset_data_fixed += data_len;
                        }
                        None => {
                            offset_slot_var += Slot::size_of() as u32;
                            let slot = Slot::new(offset_slot_var, Slot::size_of() as u32);
                            slot_all.push(FieldDesc::new(
                                slot.clone(),
                                ty.clone(),
                                false,
                            ));
                            offset_len_slot_var.push(FieldDesc::new(
                                slot,
                                ty.clone(),
                                false,
                            ));
                        }
                    }
                }
                Err(e) => {
                    panic!("get type length error, {:?}", e);
                }
            }
        }
        Ok(Self {
            offset_len_data_fixed,
            offset_len_slot_var,
            slot_all,
            fixed_count,
            var_count,
            total_fixed_size,
            type_desc,
        })
    }

    pub fn normalized_type_desc_vec<T: Default + Clone + 'static>(vec: Vec<(DatType, T)>) -> RS<(Vec<DatType>, Vec<T>)> {
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
fn _normalized<T: Default + Clone + 'static>(vec_type_desc: Vec<(DatType, T)>) -> RS<(Vec<DatType>, Vec<T>)> {
    let mut vec = vec_type_desc;

    // Collect all comparison results first to handle errors
    let mut indices: Vec<usize> = (0..vec.len()).collect();

    let mut err: Option<MError> = None;
    indices.sort_by(|&i, &j| {
        let e_i = &vec[i].0;
        let e_j = &vec[j].0;
        match e_i.cmp_ord(e_j) {
            Ok(ordering) => ordering,
            Err(e) => {
                // Handle error - you might want to panic, log, or use a fallback
                err = Some(e);
                Ordering::Equal // Fallback ordering
            }
        }
    });
    match err {
        Some(e) => { return Err(e) }
        None => {}
    }
    let mut sorted_vec = vec![];
    let mut payload_vec = vec![];
    for index in indices {
        let ty = mem::take(&mut vec[index].0);
        let pl = mem::take(&mut vec[index].1);
        sorted_vec.push(ty);
        payload_vec.push(pl);
    }
    Ok((sorted_vec, payload_vec))
}

fn is_normalized(vec_type_desc: &[DatType]) -> RS<bool> {
    for i in 0..vec_type_desc.len() {
        if i + 1 < vec_type_desc.len() && vec_type_desc[i].cmp_ord(&vec_type_desc[i + 1])?.is_gt() {
            return Ok(false);
        }
    }
    Ok(true)
}


#[cfg(test)]
mod tests {
    use crate::tuple::tuple_binary_desc::TupleBinaryDesc;
    use mudu_type::dat_type::DatType;
    use mudu_type::dat_type_id::DatTypeID;
    #[test]
    fn test_tuple_desc() {
        let dat_types = vec![
            DatType::new_no_param(DatTypeID::F32),
            DatType::new_no_param(DatTypeID::I32),
            DatType::new_no_param(DatTypeID::F64),
            DatType::default_for(DatTypeID::String),
            DatType::new_no_param(DatTypeID::I64),
            DatType::new_no_param(DatTypeID::I32),
            DatType::new_no_param(DatTypeID::F32),
        ];
        let dat_type_and_index:Vec<(DatType, usize)> = dat_types.into_iter().enumerate()
            .map(|(i, ty)| (ty, i))
            .collect::<Vec<_>>();
        let (norm_types, _index) = TupleBinaryDesc::normalized_type_desc_vec(dat_type_and_index).unwrap();

        let _desc = TupleBinaryDesc::from(norm_types).unwrap();
    }
}