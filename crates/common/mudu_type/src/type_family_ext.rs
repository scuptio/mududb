//! Extension trait providing the per-family function-table accessors that used
//! to live directly on [`TypeFamily`].
//!
//! Keeping these accessors in a trait (rather than inherent methods) lets the
//! [`TypeFamily`] enum stay a pure definition with no dependency on the
//! conversion/comparison function types or the function table, while callers
//! keep calling `family.fn_recv()` etc. by importing this trait.

use crate::data_type_fn_compare::{FnCompare, FnEqual, FnHash, FnOrder};
use crate::data_type_fn_convert::{
    FnBase, FnDataLength, FnDefault, FnInputJson, FnInputMsgPack, FnInputTextual, FnOutputJson,
    FnOutputMsgPack, FnOutputTextual, FnReceive, FnSend, FnSendTo, FnTypeLength,
};
use crate::data_type_fn_param::{FnParam, FnParamDefault};
use crate::data_type_impl::data_type_table::{
    get_fn_convert, get_opt_fn_compare, get_opt_fn_param,
};
use crate::type_family::TypeFamily;

#[cfg(any(test, feature = "arbitrary"))]
use crate::data_type_fn_arbitrary::{FnArbParam, FnArbPrintable, FnArbValue, FnArbitrary};
#[cfg(any(test, feature = "arbitrary"))]
use crate::data_type_impl::data_type_table::get_fn_arbitrary;

/// Per-family function-table accessors for [`TypeFamily`].
pub trait TypeFamilyExt {
    /// Returns the base conversion function table of this family.
    fn fn_base(&self) -> &'static FnBase;

    /// Returns the comparison function table of this family, if the family
    /// supports comparison.
    fn opt_fn_compare(&self) -> &'static Option<FnCompare>;

    /// Returns the parameter function table of this family, if the family is
    /// parameterized.
    fn opt_fn_param(&self) -> &'static Option<FnParam>;

    /// Returns the textual input (parse) function.
    fn fn_input(&self) -> FnInputTextual;

    /// Returns the textual output (render) function.
    fn fn_output(&self) -> FnOutputTextual;

    /// Returns the JSON input function.
    fn fn_input_json(&self) -> FnInputJson;

    /// Returns the JSON output function.
    fn fn_output_json(&self) -> FnOutputJson;

    /// Returns the MessagePack input function.
    fn fn_input_msg_pack(&self) -> FnInputMsgPack;

    /// Returns the MessagePack output function.
    fn fn_output_msg_pack(&self) -> FnOutputMsgPack;

    /// Returns the function computing the binary length of the type itself.
    fn fn_send_type_len(&self) -> FnTypeLength;

    /// Returns the function computing the binary length of a value.
    fn fn_send_data_len(&self) -> FnDataLength;

    /// Returns the binary receive (decode) function.
    fn fn_recv(&self) -> FnReceive;

    /// Returns the binary send (encode) function.
    fn fn_send(&self) -> FnSend;

    /// Returns the binary send function that writes into a caller-provided
    /// buffer.
    fn fn_send_to(&self) -> FnSendTo;

    /// Returns the function producing the default value of a type.
    fn fn_default(&self) -> FnDefault;

    /// Returns the ordering function, if the family supports ordering.
    fn fn_order(&self) -> Option<FnOrder>;

    /// Returns the equality function, if the family supports equality.
    fn fn_equal(&self) -> Option<FnEqual>;

    /// Returns the hash function, if the family supports hashing.
    fn fn_hash(&self) -> Option<FnHash>;

    /// Returns the default-parameter function, if the family is
    /// parameterized.
    fn fn_param_default(&self) -> Option<FnParamDefault>;

    /// Returns the arbitrary value-generation function table of this family
    /// (test support).
    #[cfg(any(test, feature = "arbitrary"))]
    fn fn_arbitrary(&self) -> &'static FnArbitrary;

    /// Returns the function generating an arbitrary type parameter
    /// (test support).
    #[cfg(any(test, feature = "arbitrary"))]
    fn fn_arb_param(&self) -> FnArbParam;

    /// Returns the function generating an arbitrary internal value
    /// (test support).
    #[cfg(any(test, feature = "arbitrary"))]
    fn fn_arb_internal(&self) -> FnArbValue;

    /// Returns the function generating an arbitrary printable value
    /// (test support).
    #[cfg(any(test, feature = "arbitrary"))]
    fn fn_arb_printable(&self) -> FnArbPrintable;
}

impl TypeFamilyExt for TypeFamily {
    fn fn_base(&self) -> &'static FnBase {
        get_fn_convert(self.to_u32())
    }

    fn opt_fn_compare(&self) -> &'static Option<FnCompare> {
        get_opt_fn_compare(self.to_u32())
    }

    fn opt_fn_param(&self) -> &'static Option<FnParam> {
        get_opt_fn_param(self.to_u32())
    }

    fn fn_input(&self) -> FnInputTextual {
        self.fn_base().input_textual
    }

    fn fn_output(&self) -> FnOutputTextual {
        self.fn_base().output_textual
    }

    fn fn_input_json(&self) -> FnInputJson {
        self.fn_base().input_json
    }

    fn fn_output_json(&self) -> FnOutputJson {
        self.fn_base().output_json
    }

    fn fn_input_msg_pack(&self) -> FnInputMsgPack {
        self.fn_base().input_msg_pack
    }

    fn fn_output_msg_pack(&self) -> FnOutputMsgPack {
        self.fn_base().output_msg_pack
    }

    fn fn_send_type_len(&self) -> FnTypeLength {
        self.fn_base().type_len
    }

    fn fn_send_data_len(&self) -> FnDataLength {
        self.fn_base().data_len
    }

    fn fn_recv(&self) -> FnReceive {
        self.fn_base().receive
    }

    fn fn_send(&self) -> FnSend {
        self.fn_base().send
    }

    fn fn_send_to(&self) -> FnSendTo {
        self.fn_base().send_to
    }

    fn fn_default(&self) -> FnDefault {
        self.fn_base().default
    }

    fn fn_order(&self) -> Option<FnOrder> {
        self.opt_fn_compare().as_ref().map(|compare| compare.order)
    }

    fn fn_equal(&self) -> Option<FnEqual> {
        self.opt_fn_compare().as_ref().map(|compare| compare.equal)
    }

    fn fn_hash(&self) -> Option<FnHash> {
        self.opt_fn_compare().as_ref().map(|compare| compare.hash)
    }

    fn fn_param_default(&self) -> Option<FnParamDefault> {
        self.opt_fn_param().as_ref().and_then(|param| param.default)
    }

    #[cfg(any(test, feature = "arbitrary"))]
    fn fn_arbitrary(&self) -> &'static FnArbitrary {
        get_fn_arbitrary(self.to_u32())
    }

    #[cfg(any(test, feature = "arbitrary"))]
    fn fn_arb_param(&self) -> FnArbParam {
        self.fn_arbitrary().param
    }

    #[cfg(any(test, feature = "arbitrary"))]
    fn fn_arb_internal(&self) -> FnArbValue {
        self.fn_arbitrary().value_object
    }

    #[cfg(any(test, feature = "arbitrary"))]
    fn fn_arb_printable(&self) -> FnArbPrintable {
        self.fn_arbitrary().value_print
    }
}
