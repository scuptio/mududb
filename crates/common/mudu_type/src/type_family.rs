use crate::type_family_meta::{get_dt_name, get_kind, is_all_fixed_len};
use crate::type_kind::TypeKind;

#[cfg(any(test, feature = "arbitrary"))]
use arbitrary::Arbitrary;

use serde::{Deserialize, Serialize};

/// Type family identifier.
///
/// A `TypeFamily` identifies a *family* of concrete data types that share the same
/// implementation: the same textual/binary conversion functions, the same comparison
/// semantics, the same parameter handling, and the same in-memory object representation
/// (`DataObject`).
///
/// It is **not** a unique identity for a fully-instantiated type.  Two concrete types
/// such as `varchar(42)` and `varchar(255)`, or `array<integer>` and `array<string>`,
/// belong to the same `TypeFamily` (`String` or `Array`) while differing in their
/// parameters.  The fully-qualified concrete type is represented by [`crate::data_type::DataType`], which
/// pairs a `TypeFamily` with an optional parameter object (`DataTypeParamKind`).
///
/// # Design rationale
///
/// This design deliberately separates the *implementation family* from the *concrete
/// type identity*.  Multiple related concrete types may delegate to a single shared
/// function implementation, and `TypeFamily` is the abstraction that groups those types.
///
/// # Relationship to `TypeKind`
///
/// * `TypeFamily` answers "which implementation family does this type belong to?"
///   (`String`, `Numeric`, `Array`, ...).
/// * `TypeKind` answers "what is the structural class of this family?"  (`Scalar`,
///   `Array`, `Struct`, `Binary`).
#[repr(u32)]
#[derive(Hash, Eq, Ord, PartialEq, PartialOrd, Copy, Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "arbitrary"), derive(Arbitrary))]
pub enum TypeFamily {
    /// 32-bit signed integer (`int`).
    I32 = 0,
    /// 64-bit signed integer (`bigint`).
    I64 = 1,
    /// 32-bit floating point (`float`).
    F32 = 2,
    /// 64-bit floating point (`double`).
    F64 = 3,
    /// Variable-length string (`varchar`), parameterized by maximum length.
    String = 4,
    /// 128-bit unsigned integer used as object identifier (`oid`).
    U128 = 5,
    /// 128-bit signed integer (`i128`).
    I128 = 6,
    /// Fixed-point decimal (`numeric`), parameterized by precision and scale.
    Numeric = 7,
    /// Calendar date (`date`).
    Date = 8,
    /// Time of day (`time`), parameterized by fractional-second precision.
    Time = 9,
    /// Timestamp without time zone (`timestamp`), parameterized by
    /// fractional-second precision.
    Timestamp = 10,
    /// Timestamp with time zone (`timestamptz`), parameterized by
    /// fractional-second precision.
    TimestampTz = 11,

    /// Array of values of a single element type (`array`).
    Array = 1001,
    /// Struct with named fields (`object`).
    Struct = 1002,
    /// Opaque binary data (`binary`).
    Binary = 1003,
}

// Cache the maximum ID for efficient access
const MAX_ID: TypeFamily = TypeFamily::Binary;

impl TypeFamily {
    /// Returns the maximum valid TypeFamily value as u32
    pub fn max() -> u32 {
        MAX_ID.to_u32()
    }

    /// Converts the enum variant to its underlying u32 representation
    pub fn to_u32(&self) -> u32 {
        *self as u32
    }

    /// Creates a TypeFamily from a u32 value
    ///
    /// # Safety
    /// Caller must ensure the value corresponds to a valid TypeFamily variant
    pub fn from_u32(n: u32) -> TypeFamily {
        unsafe { std::mem::transmute(n) }
    }

    /// Returns true if every type of this family has a fixed binary length.
    pub fn is_fixed_len(&self) -> bool {
        is_all_fixed_len(self.to_u32())
    }

    /// Returns the SQL type name of this family.
    pub fn name(&self) -> &str {
        get_dt_name(self.to_u32())
    }

    /// Returns the structural kind (scalar, array, record, binary) of this
    /// family.
    pub fn kind(&self) -> TypeKind {
        get_kind(self.to_u32())
    }

    /// Returns true if this family is a scalar type.
    pub fn is_scalar_type(&self) -> bool {
        self.kind() == TypeKind::Scalar
    }

    /// Alias of [`TypeFamily::kind`].
    pub fn dat_kind(&self) -> TypeKind {
        self.kind()
    }

    /// Returns true if types of this family accept a parameter object.
    pub fn has_param(&self) -> bool {
        !matches!(
            self,
            TypeFamily::I32
                | TypeFamily::I64
                | TypeFamily::I128
                | TypeFamily::F32
                | TypeFamily::F64
                | TypeFamily::U128
                | TypeFamily::Date
        )
    }
}

/// Naming of a concrete type: returns the SQL type name.
pub trait TypeName {
    /// Returns the SQL type name.
    fn name(&self) -> String;
}

#[cfg(test)]
mod tests {
    use super::TypeFamily;
    use crate::type_kind::TypeKind;

    #[test]
    fn to_u32_from_u32_roundtrip_for_all_variants() {
        let variants = [
            TypeFamily::I32,
            TypeFamily::I64,
            TypeFamily::F32,
            TypeFamily::F64,
            TypeFamily::String,
            TypeFamily::U128,
            TypeFamily::I128,
            TypeFamily::Numeric,
            TypeFamily::Date,
            TypeFamily::Time,
            TypeFamily::Timestamp,
            TypeFamily::TimestampTz,
            TypeFamily::Array,
            TypeFamily::Struct,
            TypeFamily::Binary,
        ];
        for v in variants {
            assert_eq!(
                TypeFamily::from_u32(v.to_u32()),
                v,
                "roundtrip failed for {v:?}"
            );
        }
    }

    #[test]
    fn max_matches_binary() {
        assert_eq!(TypeFamily::max(), TypeFamily::Binary.to_u32());
    }

    #[test]
    fn is_scalar_type_true_for_scalars_false_for_complex() {
        assert!(TypeFamily::I32.is_scalar_type());
        assert!(TypeFamily::I64.is_scalar_type());
        assert!(TypeFamily::F32.is_scalar_type());
        assert!(TypeFamily::F64.is_scalar_type());
        assert!(TypeFamily::String.is_scalar_type());
        assert!(TypeFamily::U128.is_scalar_type());
        assert!(TypeFamily::I128.is_scalar_type());
        assert!(TypeFamily::Numeric.is_scalar_type());
        assert!(TypeFamily::Date.is_scalar_type());
        assert!(TypeFamily::Time.is_scalar_type());
        assert!(TypeFamily::Timestamp.is_scalar_type());
        assert!(TypeFamily::TimestampTz.is_scalar_type());
        assert!(!TypeFamily::Array.is_scalar_type());
        assert!(!TypeFamily::Struct.is_scalar_type());
        assert!(!TypeFamily::Binary.is_scalar_type());
    }

    #[test]
    fn has_param_matches_expectations() {
        let has_param = [
            TypeFamily::String,
            TypeFamily::Numeric,
            TypeFamily::Time,
            TypeFamily::Timestamp,
            TypeFamily::TimestampTz,
        ];
        let no_param = [
            TypeFamily::I32,
            TypeFamily::I64,
            TypeFamily::I128,
            TypeFamily::F32,
            TypeFamily::F64,
            TypeFamily::U128,
            TypeFamily::Date,
        ];
        for v in has_param {
            assert!(v.has_param(), "{v:?} should have parameter");
        }
        for v in no_param {
            assert!(!v.has_param(), "{v:?} should not have parameter");
        }
    }

    #[test]
    fn kind_returns_correct_kind() {
        assert_eq!(TypeFamily::I32.kind(), TypeKind::Scalar);
        assert_eq!(TypeFamily::String.kind(), TypeKind::Scalar);
        assert_eq!(TypeFamily::Array.kind(), TypeKind::Array);
        assert_eq!(TypeFamily::Struct.kind(), TypeKind::Struct);
        assert_eq!(TypeFamily::Binary.kind(), TypeKind::Binary);
    }

    #[test]
    fn dat_kind_returns_correct_kind() {
        assert_eq!(TypeFamily::I32.dat_kind(), TypeKind::Scalar);
        assert_eq!(TypeFamily::String.dat_kind(), TypeKind::Scalar);
        assert_eq!(TypeFamily::Array.dat_kind(), TypeKind::Array);
        assert_eq!(TypeFamily::Struct.dat_kind(), TypeKind::Struct);
        assert_eq!(TypeFamily::Binary.dat_kind(), TypeKind::Binary);
    }
}
