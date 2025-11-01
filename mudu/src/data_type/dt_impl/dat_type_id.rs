#[cfg(any(test, feature = "test"))]
use crate::data_type::dt_fn_arbitrary::{FnArbParam, FnArbPrintable, FnArbValue, FnArbitrary};
use crate::data_type::dt_fn_compare::{FnCompare, FnEqual, FnHash, FnOrder};
use crate::data_type::dt_fn_convert::{
    FnBase, FnDefault, FnFromTyped, FnInput, FnOutput, FnRecv, FnSend, FnSendTo, FnToTyped,
};
#[cfg(any(test, feature = "test"))]
use crate::data_type::dt_impl::dat_table::get_fn_arbitrary;
use crate::data_type::dt_impl::dat_table::{
    get_dt_name, get_fn_convert, get_opt_fn_compare, get_opt_fn_param, is_fixed_len, type_len,
};
use crate::data_type::dt_param::{FnParam, FnParamDefault};
use crate::data_type::param_obj::ParamObj;
#[cfg(any(test, feature = "test"))]
use arbitrary::Arbitrary;
use serde::{Deserialize, Serialize};

#[repr(u32)]
#[derive(Hash, Eq, Ord, PartialEq, PartialOrd, Copy, Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "test"), derive(Arbitrary))]
pub enum DatTypeID {
    I32 = 0,
    I64 = 1,
    F32 = 2,
    F64 = 3,
    CharVarLen = 4,
    CharFixedLen = 5,
}

const THE_MAX_ID: DatTypeID = DatTypeID::CharFixedLen;

impl DatTypeID {
    pub fn max() -> u32 {
        THE_MAX_ID.to_u32()
    }

    pub fn to_u32(&self) -> u32 {
        *self as u32
    }

    pub fn from_u32(n: u32) -> DatTypeID {
        unsafe { std::mem::transmute::<_, Self>(n) }
    }

    pub fn fn_base(&self) -> &'static FnBase {
        get_fn_convert(self.to_u32())
    }

    pub fn opt_fn_compare(&self) -> &'static Option<FnCompare> {
        get_opt_fn_compare(self.to_u32())
    }

    #[cfg(any(test, feature = "test"))]
    pub fn fn_arbitrary(&self) -> &'static FnArbitrary {
        get_fn_arbitrary(self.to_u32())
    }

    pub fn opt_fn_param(&self) -> &'static Option<FnParam> {
        get_opt_fn_param(self.to_u32())
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

    pub fn fn_default(&self) -> FnDefault {
        self.fn_base().default
    }

    pub fn fn_order(&self) -> Option<FnOrder> {
        self.opt_fn_compare().as_ref().map(|n| n.order)
    }

    pub fn fn_equal(&self) -> Option<FnEqual> {
        self.opt_fn_compare().as_ref().map(|n| n.equal)
    }

    pub fn fn_hash(&self) -> Option<FnHash> {
        self.opt_fn_compare().as_ref().map(|n| n.hash)
    }

    #[cfg(any(test, feature = "test"))]
    pub fn fn_arb_param(&self) -> FnArbParam {
        self.fn_arbitrary().param
    }
    #[cfg(any(test, feature = "test"))]
    pub fn fn_arb_typed(&self) -> FnArbValue {
        self.fn_arbitrary().value_typed
    }
    #[cfg(any(test, feature = "test"))]
    pub fn fn_arb_printable(&self) -> FnArbPrintable {
        self.fn_arbitrary().value_print
    }

    pub fn fn_param_default(&self) -> Option<FnParamDefault> {
        self.opt_fn_param().as_ref().map(|p| p.default)
    }

    pub fn is_primitive_type(&self) -> bool {
        match self {
            DatTypeID::I32 | DatTypeID::I64 | DatTypeID::F32 | DatTypeID::F64 => true,
            _ => false,
        }
    }
}
