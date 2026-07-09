use crate::data_type_fn_compare::{FnCompare, FnEqual, FnHash, FnOrder};
use crate::data_type_fn_convert::{
    FnBase, FnDataLength, FnDefault, FnInputJson, FnInputMsgPack, FnInputTextual, FnOutputJson,
    FnOutputMsgPack, FnOutputTextual, FnReceive, FnSend, FnSendTo, FnTypeLength,
};
use crate::data_type_fn_param::{FnParam, FnParamDefault};
use crate::data_type_impl::data_type_table::{
    get_dt_name, get_fn_convert, get_kind, get_opt_fn_compare, get_opt_fn_param, is_all_fixed_len,
};
use crate::type_kind::TypeKind;

#[cfg(any(test, feature = "test"))]
use crate::data_type_fn_arbitrary::{FnArbParam, FnArbPrintable, FnArbValue, FnArbitrary};
#[cfg(any(test, feature = "test"))]
use crate::data_type_impl::data_type_table::get_fn_arbitrary;
#[cfg(any(test, feature = "test"))]
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
/// parameters.  The fully-qualified concrete type is represented by [`DataType`], which
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
///   `Array`, `Record`, `Binary`).
#[repr(u32)]
#[derive(Hash, Eq, Ord, PartialEq, PartialOrd, Copy, Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "test"), derive(Arbitrary))]
pub enum TypeFamily {
    I32 = 0,
    I64 = 1,
    F32 = 2,
    F64 = 3,
    String = 4,
    U128 = 5,
    I128 = 6,
    Numeric = 7,
    Date = 8,
    Time = 9,
    Timestamp = 10,
    TimestampTz = 11,

    Array = 1001,
    Record = 1002,
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

    // Core function accessors
    pub fn fn_base(&self) -> &'static FnBase {
        get_fn_convert(self.to_u32())
    }

    pub fn opt_fn_compare(&self) -> &'static Option<FnCompare> {
        get_opt_fn_compare(self.to_u32())
    }

    pub fn opt_fn_param(&self) -> &'static Option<FnParam> {
        get_opt_fn_param(self.to_u32())
    }

    // Type information queries
    pub fn is_fixed_len(&self) -> bool {
        is_all_fixed_len(self.to_u32())
    }

    pub fn name(&self) -> &str {
        get_dt_name(self.to_u32())
    }

    // Conversion function accessors
    pub fn fn_input(&self) -> FnInputTextual {
        self.fn_base().input_textual
    }

    pub fn fn_output(&self) -> FnOutputTextual {
        self.fn_base().output_textual
    }

    pub fn fn_input_json(&self) -> FnInputJson {
        self.fn_base().input_json
    }

    pub fn fn_output_json(&self) -> FnOutputJson {
        self.fn_base().output_json
    }

    pub fn fn_input_msg_pack(&self) -> FnInputMsgPack {
        self.fn_base().input_msg_pack
    }

    pub fn fn_output_msg_pack(&self) -> FnOutputMsgPack {
        self.fn_base().output_msg_pack
    }

    pub fn fn_send_type_len(&self) -> FnTypeLength {
        self.fn_base().type_len
    }

    pub fn fn_send_data_len(&self) -> FnDataLength {
        self.fn_base().data_len
    }

    pub fn fn_recv(&self) -> FnReceive {
        self.fn_base().receive
    }

    pub fn fn_send(&self) -> FnSend {
        self.fn_base().send
    }

    pub fn fn_send_to(&self) -> FnSendTo {
        self.fn_base().send_to
    }

    pub fn fn_default(&self) -> FnDefault {
        self.fn_base().default
    }

    // Comparison function accessors
    pub fn fn_order(&self) -> Option<FnOrder> {
        self.opt_fn_compare().as_ref().map(|compare| compare.order)
    }

    pub fn fn_equal(&self) -> Option<FnEqual> {
        self.opt_fn_compare().as_ref().map(|compare| compare.equal)
    }

    pub fn fn_hash(&self) -> Option<FnHash> {
        self.opt_fn_compare().as_ref().map(|compare| compare.hash)
    }

    // Parameter function accessors
    pub fn fn_param_default(&self) -> Option<FnParamDefault> {
        self.opt_fn_param().as_ref().and_then(|param| param.default)
    }

    // Type classification
    pub fn kind(&self) -> TypeKind {
        get_kind(self.to_u32())
    }

    pub fn is_scalar_type(&self) -> bool {
        self.kind() == TypeKind::Scalar
    }

    pub fn dat_kind(&self) -> TypeKind {
        self.kind()
    }

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

    // Test/arbitrary function accessors (conditionally compiled)
    #[cfg(any(test, feature = "test"))]
    pub fn fn_arbitrary(&self) -> &'static FnArbitrary {
        get_fn_arbitrary(self.to_u32())
    }

    #[cfg(any(test, feature = "test"))]
    pub fn fn_arb_param(&self) -> FnArbParam {
        self.fn_arbitrary().param
    }

    #[cfg(any(test, feature = "test"))]
    pub fn fn_arb_internal(&self) -> FnArbValue {
        self.fn_arbitrary().value_object
    }

    #[cfg(any(test, feature = "test"))]
    pub fn fn_arb_printable(&self) -> FnArbPrintable {
        self.fn_arbitrary().value_print
    }
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
            TypeFamily::Record,
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
        assert!(!TypeFamily::Record.is_scalar_type());
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
        assert_eq!(TypeFamily::Record.kind(), TypeKind::Record);
        assert_eq!(TypeFamily::Binary.kind(), TypeKind::Binary);
    }

    #[test]
    fn dat_kind_returns_correct_kind() {
        assert_eq!(TypeFamily::I32.dat_kind(), TypeKind::Scalar);
        assert_eq!(TypeFamily::String.dat_kind(), TypeKind::Scalar);
        assert_eq!(TypeFamily::Array.dat_kind(), TypeKind::Array);
        assert_eq!(TypeFamily::Record.dat_kind(), TypeKind::Record);
        assert_eq!(TypeFamily::Binary.dat_kind(), TypeKind::Binary);
    }
}
