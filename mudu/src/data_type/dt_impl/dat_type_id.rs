use crate::data_type::dt_fn_arbitrary::{FnArbParam, FnArbPrintable, FnArbValue, FnArbitrary};
use crate::data_type::dt_fn_base::{
    FnBase, FnFromTyped, FnInput, FnOutput, FnRecv, FnSend, FnSendTo, FnToTyped,
};
use crate::data_type::dt_fn_compare::{FnCompare, FnEqual, FnHash, FnOrder};
use crate::data_type::dt_impl::dat_table::{
    get_arbitrary_function, get_compare_function, get_convert_function, get_dt_name, is_fixed_len,
    type_len,
};
use crate::data_type::dt_param::ParamObj;
use arbitrary::Arbitrary;
use serde::{Deserialize, Serialize};

#[repr(u32)]
#[derive(
    Arbitrary, Hash, Eq, Ord, PartialEq, PartialOrd, Copy, Clone, Debug, Serialize, Deserialize,
)]
pub enum DatTypeID {
    I32 = 0,
    I64 = 1,
    F32 = 2,
    F64 = 3,
    FixedLenString = 4,
    VarLenString = 5,
}

impl DatTypeID {
    pub fn to_u32(&self) -> u32 {
        *self as u32
    }
    pub fn from_u32(n: u32) -> DatTypeID {
        unsafe { std::mem::transmute::<_, Self>(n) }
    }

    pub fn fn_base(&self) -> &'static FnBase {
        get_convert_function(self.to_u32())
    }

    pub fn fn_compare(&self) -> &'static Option<FnCompare> {
        get_compare_function(self.to_u32())
    }

    pub fn fn_arbitrary(&self) -> &'static FnArbitrary {
        get_arbitrary_function(self.to_u32())
    }

    pub fn is_fixed_len(&self) -> bool {
        is_fixed_len(self.to_u32())
    }

    pub fn type_len(&self, opt_params: &ParamObj) -> Option<usize> {
        type_len(self.to_u32(), opt_params)
    }

    pub fn name(&self) -> &str {
        get_dt_name(self.to_u32())
    }

    pub fn fn_input(&self) -> FnInput {
        self.fn_base().input
    }

    pub fn fn_output(&self) -> FnOutput {
        self.fn_base().output
    }
    pub fn fn_recv(&self) -> FnRecv {
        self.fn_base().recv
    }
    pub fn fn_send(&self) -> FnSend {
        self.fn_base().send
    }
    pub fn fn_send_to(&self) -> FnSendTo {
        self.fn_base().send_to
    }

    pub fn fn_to_typed(&self) -> FnToTyped {
        self.fn_base().to_typed
    }

    pub fn fn_from_typed(&self) -> FnFromTyped {
        self.fn_base().from_typed
    }

    pub fn fn_order(&self) -> Option<FnOrder> {
        self.fn_compare().as_ref().map(|n| n.order)
    }

    pub fn fn_equal(&self) -> Option<FnEqual> {
        self.fn_compare().as_ref().map(|n| n.equal)
    }

    pub fn fn_hash(&self) -> Option<FnHash> {
        self.fn_compare().as_ref().map(|n| n.hash)
    }

    pub fn fn_arb_param(&self) -> FnArbParam {
        self.fn_arbitrary().param
    }

    pub fn fn_arb_typed(&self) -> FnArbValue {
        self.fn_arbitrary().value_typed
    }

    pub fn fn_arb_printable(&self) -> FnArbPrintable {
        self.fn_arbitrary().value_print
    }
}
