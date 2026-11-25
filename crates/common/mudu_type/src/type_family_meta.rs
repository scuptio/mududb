//! Pure metadata for [`TypeFamily`]: the structural kind, SQL name and
//! fixed-length classification, indexed by the family's numeric id.
//!
//! Kept separate from the function table (`data_type_impl::data_type_table`)
//! and expressed with numeric literals, so the [`TypeFamily`] definition can
//! answer these queries without depending on any conversion/comparison
//! function types.

use crate::type_kind::TypeKind;

/// Returns the structural kind of the family identified by `id`.
pub fn get_kind(id: u32) -> TypeKind {
    match id {
        0..=12 => TypeKind::Scalar,
        1001 => TypeKind::Array,
        1002 => TypeKind::Struct,
        1003 => TypeKind::Binary,
        _ => TypeKind::Scalar,
    }
}

/// Returns the SQL name of the family identified by `id`.
pub fn get_dt_name(id: u32) -> &'static str {
    match id {
        0 => "int",
        1 => "bigint",
        2 => "float",
        3 => "double",
        4 => "varchar",
        5 => "oid",
        6 => "i128",
        7 => "numeric",
        8 => "date",
        9 => "time",
        10 => "timestamp",
        11 => "timestamptz",
        1001 => "array",
        1002 => "object",
        1003 => "binary",
        _ => "",
    }
}

/// Returns true if every subtype of the family identified by `id` has a fixed
/// binary length.
pub fn is_all_fixed_len(id: u32) -> bool {
    match id {
        0 | 1 | 2 | 3 | 5 | 6 | 7 | 8 | 9 | 10 | 11 => true,
        4 | 1001 | 1002 | 1003 => false,
        _ => false,
    }
}
