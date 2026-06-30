//! `tuple::mod` module.
#![allow(missing_docs)]

mod binary_to_json;

#[cfg(test)]
mod binary_to_json_test;

pub mod bitmap;
#[cfg(test)]
mod bitmap_test;
pub mod build_tuple;
#[cfg(test)]
mod build_tuple_test;
pub mod comparator;
#[cfg(test)]
mod comparator_test;
pub mod datum_convert;
#[cfg(test)]
mod datum_convert_test;
pub mod datum_desc;
#[cfg(test)]
mod datum_desc_test;
pub mod datum_vec;
#[cfg(test)]
mod datum_vec_test;

pub mod enumerable_datum;
mod field_desc;
#[cfg(test)]
mod field_desc_test;
pub mod migrate;
pub mod nullable_tuple;
#[cfg(test)]
mod nullable_tuple_test;
mod read_datum;
#[cfg(test)]
mod read_datum_test;
mod slot;
pub mod tuple_binary;
pub mod tuple_binary_desc;
#[cfg(test)]
mod tuple_binary_desc_test;
pub mod tuple_datum;
#[cfg(test)]
mod tuple_datum_test;
pub mod tuple_field;
pub mod tuple_field_desc;
#[cfg(test)]
mod tuple_field_desc_test;
#[cfg(test)]
mod tuple_field_test;
pub mod tuple_key;
#[cfg(test)]
mod tuple_key_test;
pub mod tuple_ref;
#[cfg(test)]
mod tuple_ref_test;

pub mod tuple_value;
#[cfg(test)]
mod tuple_value_test;
pub mod typed_bin;
#[cfg(test)]
mod typed_bin_test;

pub mod update_tuple;
#[cfg(test)]
mod update_tuple_test;
pub mod vec_dyn_datum;
#[cfg(test)]
mod vec_dyn_datum_test;
mod write_value;
#[cfg(test)]
mod write_value_test;
