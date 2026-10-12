//! Tuple layout, encoding, and conversion utilities.

/// Conversion of tuple binary fields to JSON values.
mod binary_to_json;

#[cfg(test)]
mod binary_to_json_test;

/// Bitmap utilities for the tuple null-bitmap.
pub mod bitmap;
#[cfg(test)]
mod bitmap_test;
/// Building of binary tuples from field values.
pub mod build_tuple;
#[cfg(test)]
mod build_tuple_test;
/// Comparison, equality, and hashing of binary tuples.
pub mod comparator;
#[cfg(test)]
mod comparator_test;
/// Conversion between datums, values, and binaries.
pub mod datum_convert;
#[cfg(test)]
mod datum_convert_test;
/// Description of a named, typed datum.
pub mod datum_desc;
#[cfg(test)]
mod datum_desc_test;
/// Conversions between datum vectors and value vectors.
pub mod datum_vec;
#[cfg(test)]
mod datum_vec_test;

/// Datum types enumerable as a sequence of fields.
pub mod enumerable_datum;
/// Per-field storage layout descriptor of a binary tuple.
mod field_desc;
#[cfg(test)]
mod field_desc_test;
/// Migration handlers for the tuple binary format.
pub mod migrate;
/// Nullable-aware tuple field access.
pub mod nullable_tuple;
#[cfg(test)]
mod nullable_tuple_test;
/// Reading of fixed- and variable-length values from a binary tuple.
mod read_datum;
#[cfg(test)]
mod read_datum_test;
/// Offset/length slot pointing into a binary tuple.
mod slot;
/// Binary tuple byte layout types.
pub mod tuple_binary;
/// Storage layout descriptor of a binary tuple.
pub mod tuple_binary_desc;
#[cfg(test)]
mod tuple_binary_desc_test;
/// Rust tuple to/from binary and value conversions.
pub mod tuple_datum;
#[cfg(test)]
mod tuple_datum_test;
/// A tuple of per-field binary values.
pub mod tuple_field;
/// Description of a tuple's fields.
pub mod tuple_field_desc;
#[cfg(test)]
mod tuple_field_desc_test;
#[cfg(test)]
mod tuple_field_test;
/// A binary tuple used as an ordered map key.
pub mod tuple_key;
#[cfg(test)]
mod tuple_key_test;
/// Borrowed accessor over a binary tuple.
pub mod tuple_ref;
#[cfg(test)]
mod tuple_ref_test;

/// A tuple of per-field memory values.
pub mod tuple_value;
#[cfg(test)]
mod tuple_value_test;
/// A value paired with its data type, encoded to binary.
pub mod typed_bin;
#[cfg(test)]
mod typed_bin_test;

/// In-place update of fields in a binary tuple.
pub mod update_tuple;
#[cfg(test)]
mod update_tuple_test;
/// Vectors of dynamic datum trait objects.
pub mod vec_dyn_datum;
#[cfg(test)]
mod vec_dyn_datum_test;
/// Writing of values into a binary tuple.
mod write_value;
#[cfg(test)]
mod write_value_test;
