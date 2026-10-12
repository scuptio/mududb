//! Storage layout descriptor of a binary tuple.

use crate::tuple::bitmap::aligned_byte_len;
use crate::tuple::field_desc::FieldDesc;
use crate::tuple::slot::Slot;
use mudu::common::cmp_order::Order;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_type::data_type::DataType;
use serde::{Deserialize, Serialize};
use std::mem;

/// Storage layout of a binary tuple: where each field lives and how the
/// null bitmap and slots are arranged.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TupleBinaryDesc {
    offset_len_data_fixed: Vec<FieldDesc>,
    offset_len_slot_var: Vec<FieldDesc>,
    slot_all: Vec<FieldDesc>,
    fixed_count: usize,
    var_count: usize,
    total_fixed_size: usize,
    type_desc: Vec<DataType>,
    #[serde(default)]
    nullable_count: usize,
    #[serde(default)]
    row_format_version: u32,
}

impl TupleBinaryDesc {
    /// Builds a descriptor from field types with no nullable fields and row
    /// format version 0.
    pub fn from(type_desc: Vec<DataType>) -> RS<Self> {
        let fields = type_desc
            .into_iter()
            .map(|ty| (ty, false, None))
            .collect::<Vec<_>>();
        Self::from_typed_fields(fields, 0)
    }

    /// Builds a descriptor from `(type, nullable, null_bit_idx)` triples in
    /// normalized (sorted) type order.
    pub fn from_typed_fields(
        typed_fields: Vec<(DataType, bool, Option<u16>)>,
        row_format_version: u32,
    ) -> RS<Self> {
        let type_desc = typed_fields
            .iter()
            .map(|(ty, _, _)| ty.clone())
            .collect::<Vec<_>>();
        validate_typed_fields(&type_desc, &typed_fields)?;
        let nullable_count = count_nullable(&typed_fields);
        let (total_fixed_size, fixed_count, var_count) = measure_fields(&typed_fields)?;
        let (offset_len_data_fixed, offset_len_slot_var, slot_all) =
            build_field_descs(&typed_fields, nullable_count, var_count)?;
        Ok(Self {
            offset_len_data_fixed,
            offset_len_slot_var,
            slot_all,
            fixed_count,
            var_count,
            total_fixed_size,
            type_desc,
            nullable_count,
            row_format_version,
        })
    }

    /// Sorts `(type, payload)` pairs into normalized type order, returning the
    /// sorted types and the payloads in the same new order.
    pub fn normalized_type_desc_vec<T: Default + Clone + 'static>(
        vec: Vec<(DataType, T)>,
    ) -> RS<(Vec<DataType>, Vec<T>)> {
        _normalized(vec)
    }

    /// Returns the descriptors of the fixed-length fields.
    pub fn fixed_len_field_desc(&self) -> &Vec<FieldDesc> {
        &self.offset_len_data_fixed
    }

    /// Returns the descriptors of the variable-length fields.
    pub fn var_len_field_desc(&self) -> &Vec<FieldDesc> {
        &self.offset_len_slot_var
    }

    /// Returns the descriptors of all fields in tuple order.
    pub fn field_desc(&self) -> &Vec<FieldDesc> {
        &self.slot_all
    }

    /// Returns the total number of fields.
    pub fn field_count(&self) -> usize {
        self.type_desc.len()
    }

    /// Returns the number of fixed-length fields.
    pub fn fixed_field_count(&self) -> usize {
        self.fixed_count
    }

    /// Returns the descriptor of field `idx` in tuple order.
    pub fn get_field_desc(&self, idx: usize) -> &FieldDesc {
        &self.slot_all[idx]
    }

    /// Returns the total size of the variable-length slot area in bytes.
    pub fn total_slot_size(&self) -> usize {
        self.var_count * Slot::size_of()
    }

    /// Returns the size of the tuple metadata area (null bitmap plus slots) in bytes.
    pub fn meta_size(&self) -> usize {
        self.null_bitmap_size() + self.total_slot_size()
    }

    /// Returns the size of the null bitmap in bytes.
    pub fn null_bitmap_size(&self) -> usize {
        aligned_byte_len(self.nullable_count)
    }

    /// Returns the number of nullable fields.
    pub fn nullable_count(&self) -> usize {
        self.nullable_count
    }

    /// Returns the row format version of this layout.
    pub fn row_format_version(&self) -> u32 {
        self.row_format_version
    }

    /// Returns the total size of the fixed-length data area in bytes.
    pub fn total_fixed_data_size(&self) -> usize {
        self.total_fixed_size
    }

    /// Returns the minimum size of a tuple in bytes (metadata plus fixed data).
    pub fn min_tuple_size(&self) -> usize {
        self.meta_size() + self.total_fixed_data_size()
    }
}

fn validate_typed_fields(
    type_desc: &[DataType],
    typed_fields: &[(DataType, bool, Option<u16>)],
) -> RS<()> {
    if !is_normalized(type_desc)? {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "tuple type descriptor must be normalized"
        ));
    }
    for (_, nullable, null_bit_idx) in typed_fields {
        if *nullable != null_bit_idx.is_some() {
            return Err(mudu_error!(
                ErrorCode::Parse,
                "nullable field must have a null bit index and NOT NULL field must not"
            ));
        }
    }
    Ok(())
}

fn count_nullable(typed_fields: &[(DataType, bool, Option<u16>)]) -> usize {
    typed_fields
        .iter()
        .filter_map(|(_, _, null_bit_idx)| *null_bit_idx)
        .max()
        .map(|idx| idx as usize + 1)
        .unwrap_or(0)
}

fn measure_fields(typed_fields: &[(DataType, bool, Option<u16>)]) -> RS<(usize, usize, usize)> {
    let mut total_fixed_size: usize = 0;
    let mut fixed_count: usize = 0;
    let mut var_count: usize = 0;
    for (td, _, _) in typed_fields.iter() {
        let id = td.type_family();
        match id.fn_send_type_len()(td).map_err(|e| {
            mudu_error!(
                ErrorCode::InvalidType,
                format!("fn_send_type_len failed: {e}")
            )
        })? {
            Some(len) => {
                total_fixed_size += len as usize;
                fixed_count += 1;
            }
            None => {
                var_count += 1;
            }
        }
    }
    Ok((total_fixed_size, fixed_count, var_count))
}

fn build_field_descs(
    typed_fields: &[(DataType, bool, Option<u16>)],
    nullable_count: usize,
    var_count: usize,
) -> RS<(Vec<FieldDesc>, Vec<FieldDesc>, Vec<FieldDesc>)> {
    let offset_hdr = aligned_byte_len(nullable_count);
    let offset_slot_begin = offset_hdr;
    let mut offset_slot_var = offset_slot_begin as u32;
    let mut offset_data_fixed = (offset_slot_begin + var_count * Slot::size_of()) as u32;
    let mut offset_len_data_fixed: Vec<FieldDesc> = vec![];
    let mut offset_len_slot_var: Vec<FieldDesc> = vec![];
    let mut slot_all: Vec<FieldDesc> = vec![];
    for (ty, nullable, null_bit_idx) in typed_fields.iter() {
        let id = ty.type_family();
        match id.fn_send_type_len()(ty).map_err(|e| {
            mudu_error!(
                ErrorCode::InvalidType,
                format!("fn_send_type_len failed: {e}")
            )
        })? {
            Some(data_len) => {
                let slot = Slot::new(offset_data_fixed, data_len as _);
                slot_all.push(FieldDesc::new_with_nullability(
                    slot.clone(),
                    ty.clone(),
                    true,
                    *nullable,
                    *null_bit_idx,
                ));
                offset_len_data_fixed.push(FieldDesc::new_with_nullability(
                    slot,
                    ty.clone(),
                    true,
                    *nullable,
                    *null_bit_idx,
                ));
                offset_data_fixed += data_len;
            }
            None => {
                let slot = Slot::new(offset_slot_var, Slot::size_of() as u32);
                slot_all.push(FieldDesc::new_with_nullability(
                    slot.clone(),
                    ty.clone(),
                    false,
                    *nullable,
                    *null_bit_idx,
                ));
                offset_len_slot_var.push(FieldDesc::new_with_nullability(
                    slot,
                    ty.clone(),
                    false,
                    *nullable,
                    *null_bit_idx,
                ));
                offset_slot_var += Slot::size_of() as u32;
            }
        }
    }
    Ok((offset_len_data_fixed, offset_len_slot_var, slot_all))
}

/// return the vector after normalized and the payload T of the element in the original vector
fn _normalized<T: Default + Clone + 'static>(
    vec_type_desc: Vec<(DataType, T)>,
) -> RS<(Vec<DataType>, Vec<T>)> {
    let mut vec = vec_type_desc;

    let mut indices: Vec<usize> = (0..vec.len()).collect();
    for i in 0..indices.len() {
        for j in (i + 1)..indices.len() {
            let ord = vec[indices[i]]
                .0
                .cmp_ord(&vec[indices[j]].0)
                .map_err(|e| mudu_error!(ErrorCode::InvalidType, format!("cmp_ord failed: {e}")))?;
            if ord == std::cmp::Ordering::Greater {
                indices.swap(i, j);
            }
        }
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

fn is_normalized(vec_type_desc: &[DataType]) -> RS<bool> {
    for i in 0..vec_type_desc.len() {
        if i + 1 < vec_type_desc.len() && vec_type_desc[i].cmp_ord(&vec_type_desc[i + 1])?.is_gt() {
            return Ok(false);
        }
    }
    Ok(true)
}
