use crate::dt_fn_compare::{FnCompare, FnEqual, FnHash, FnOrder};
use crate::dt_fn_convert::{
    FnBase, FnDataLen, FnDefault, FnInputJson, FnInputMsgPack, FnInputTextual, FnOutputJson,
    FnOutputMsgPack, FnOutputTextual, FnReceive, FnSend, FnSendTo, FnTypeLen,
};
use crate::dt_fn_param::{FnParam, FnParamDefault};
use crate::dt_impl::dat_table::{
    get_dt_name, get_fn_convert, get_opt_fn_compare, get_opt_fn_param, is_all_fixed_len,
};
use crate::dt_kind::DTKind;

#[cfg(any(test, feature = "test"))]
use crate::dt_fn_arbitrary::{FnArbParam, FnArbPrintable, FnArbValue, FnArbitrary};
#[cfg(any(test, feature = "test"))]
use crate::dt_impl::dat_table::get_fn_arbitrary;
#[cfg(any(test, feature = "test"))]
use arbitrary::Arbitrary;

use serde::{Deserialize, Serialize};
use std::hint;

/// Maximum ID for scalar data types
const SCALAR_ID_MAX: u32 = 1000;

/// Data Type Identifier
///
/// Types with the same ID share the same conversion functions and in-memory object representation (DatObject).
/// Scalar types (i32, i64, f32, f64, String, Numeric, temporal types) can have default parameters.
#[repr(u32)]
#[derive(Hash, Eq, Ord, PartialEq, PartialOrd, Copy, Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "test"), derive(Arbitrary))]
pub enum DatTypeID {
    // Scalar types
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

    // Complex types (start after scalar range)
    Array = SCALAR_ID_MAX + 1,
    Record = SCALAR_ID_MAX + 2,
    Binary = SCALAR_ID_MAX + 3,
}

// Cache the maximum ID for efficient access
const MAX_ID: DatTypeID = DatTypeID::Binary;

impl DatTypeID {
    /// Returns the maximum valid DatTypeID value as u32
    pub fn max() -> u32 {
        MAX_ID.to_u32()
    }

    /// Converts the enum variant to its underlying u32 representation
    pub fn to_u32(&self) -> u32 {
        *self as u32
    }

    /// Creates a DatTypeID from a u32 value
    ///
    /// # Safety
    /// Caller must ensure the value corresponds to a valid DatTypeID variant
    pub fn from_u32(n: u32) -> DatTypeID {
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

    pub fn fn_send_type_len(&self) -> FnTypeLen {
        self.fn_base().type_len
    }

    pub fn fn_send_dat_len(&self) -> FnDataLen {
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
    pub fn is_scalar_type(&self) -> bool {
        self.to_u32() < SCALAR_ID_MAX
    }

    pub fn dat_kind(&self) -> DTKind {
        if self.is_scalar_type() {
            DTKind::Scalar
        } else {
            match self {
                DatTypeID::Array => DTKind::Array,
                DatTypeID::Record => DTKind::Record,
                DatTypeID::Binary => DTKind::Binary,
                // Safety: All enum variants are covered above
                _ => unsafe { hint::unreachable_unchecked() },
            }
        }
    }

    pub fn has_param(&self) -> bool {
        !matches!(
            self,
            DatTypeID::I32
                | DatTypeID::I64
                | DatTypeID::I128
                | DatTypeID::F32
                | DatTypeID::F64
                | DatTypeID::U128
                | DatTypeID::Date
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
    use super::DatTypeID;
    use crate::dt_kind::DTKind;

    #[test]
    fn to_u32_from_u32_roundtrip_for_all_variants() {
        let variants = [
            DatTypeID::I32,
            DatTypeID::I64,
            DatTypeID::F32,
            DatTypeID::F64,
            DatTypeID::String,
            DatTypeID::U128,
            DatTypeID::I128,
            DatTypeID::Numeric,
            DatTypeID::Date,
            DatTypeID::Time,
            DatTypeID::Timestamp,
            DatTypeID::TimestampTz,
            DatTypeID::Array,
            DatTypeID::Record,
            DatTypeID::Binary,
        ];
        for v in variants {
            assert_eq!(
                DatTypeID::from_u32(v.to_u32()),
                v,
                "roundtrip failed for {v:?}"
            );
        }
    }

    #[test]
    fn max_matches_binary() {
        assert_eq!(DatTypeID::max(), DatTypeID::Binary.to_u32());
    }

    #[test]
    fn is_scalar_type_true_for_scalars_false_for_complex() {
        assert!(DatTypeID::I32.is_scalar_type());
        assert!(DatTypeID::I64.is_scalar_type());
        assert!(DatTypeID::F32.is_scalar_type());
        assert!(DatTypeID::F64.is_scalar_type());
        assert!(DatTypeID::String.is_scalar_type());
        assert!(DatTypeID::U128.is_scalar_type());
        assert!(DatTypeID::I128.is_scalar_type());
        assert!(DatTypeID::Numeric.is_scalar_type());
        assert!(DatTypeID::Date.is_scalar_type());
        assert!(DatTypeID::Time.is_scalar_type());
        assert!(DatTypeID::Timestamp.is_scalar_type());
        assert!(DatTypeID::TimestampTz.is_scalar_type());
        assert!(!DatTypeID::Array.is_scalar_type());
        assert!(!DatTypeID::Record.is_scalar_type());
        assert!(!DatTypeID::Binary.is_scalar_type());
    }

    #[test]
    fn has_param_matches_expectations() {
        let has_param = [
            DatTypeID::String,
            DatTypeID::Numeric,
            DatTypeID::Time,
            DatTypeID::Timestamp,
            DatTypeID::TimestampTz,
        ];
        let no_param = [
            DatTypeID::I32,
            DatTypeID::I64,
            DatTypeID::I128,
            DatTypeID::F32,
            DatTypeID::F64,
            DatTypeID::U128,
            DatTypeID::Date,
        ];
        for v in has_param {
            assert!(v.has_param(), "{v:?} should have parameter");
        }
        for v in no_param {
            assert!(!v.has_param(), "{v:?} should not have parameter");
        }
    }

    #[test]
    fn dat_kind_returns_correct_kind() {
        assert_eq!(DatTypeID::I32.dat_kind(), DTKind::Scalar);
        assert_eq!(DatTypeID::String.dat_kind(), DTKind::Scalar);
        assert_eq!(DatTypeID::Array.dat_kind(), DTKind::Array);
        assert_eq!(DatTypeID::Record.dat_kind(), DTKind::Record);
        assert_eq!(DatTypeID::Binary.dat_kind(), DTKind::Binary);
    }
}
