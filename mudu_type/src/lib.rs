#![feature(box_into_inner)]
pub mod array;
pub mod dat_binary;
pub mod dat_prim;
pub mod dat_textual;
pub mod dat_type;
pub mod dat_type_id;
pub mod dat_typed;
pub mod dat_value;
pub mod datum;
#[cfg(any(test, feature = "test"))]
mod dt_fn_arbitrary;
pub mod dt_fn_compare;
pub mod dt_fn_convert;
pub mod dt_fn_param;
mod dt_impl;
pub mod dt_info;
mod dt_kind;
pub mod dt_param;
pub mod len_kind;
pub mod param;

mod dat_json;
pub mod dat_msg_pack;
pub mod dat_value_inner;
pub mod dt_function;
pub mod dt_of_datum;
pub mod dtp_array;
pub mod dtp_kind;
pub mod dtp_object;
pub mod dtp_string;
pub mod record;
pub mod string;
pub mod type_error;
//pub mod universal;
pub mod db_type;
