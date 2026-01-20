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

/// Maximum ID for primitive data types
const PRIMITIVE_ID_MAX: u32 = 1000;

/// Data Type Identifier
///
/// Types with the same ID share the same conversion functions and in-memory object representation (DatObject).
/// Primitive types (i32, i64, f32, f64, String) can have default parameters.
#[repr(u32)]
#[derive(Hash, Eq, Ord, PartialEq, PartialOrd, Copy, Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "test"), derive(Arbitrary))]
pub enum DatTypeID {
    // Primitive types
    I32 = 0,
    I64 = 1,
    F32 = 2,
    F64 = 3,
    String = 4,

    // Complex types (start after primitive range)
    Array = PRIMITIVE_ID_MAX + 1,
    Record = PRIMITIVE_ID_MAX + 2,
    Binary = PRIMITIVE_ID_MAX + 3,
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
        self.opt_fn_param()
            .as_ref()
            .and_then(|param| param.default.clone())
    }

    // Type classification
    pub fn is_primitive_type(&self) -> bool {
        self.to_u32() < PRIMITIVE_ID_MAX
    }

    pub fn dat_kind(&self) -> DTKind {
        if self.is_primitive_type() {
            DTKind::Primitive
        } else {
            match self {
                DatTypeID::Array => DTKind::Array,
                DatTypeID::Record => DTKind::Object,
                // Safety: All enum variants are covered above
                _ => unsafe { hint::unreachable_unchecked() },
            }
        }
    }

    pub fn has_param(&self) -> bool {
        match self {
            DatTypeID::I32 | DatTypeID::I64 | DatTypeID::F32 | DatTypeID::F64 => false,
            _ => true,
        }
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
