pub mod array;
pub mod dat_binary;
#[cfg(test)]
mod dat_binary_test;
pub mod dat_textual;
pub mod dat_type;
pub mod dat_type_id;
pub mod dat_typed;
#[cfg(test)]
mod dat_typed_test;
#[cfg(test)]
mod dat_val_array_test;
pub mod dat_value;
pub mod datum;
#[cfg(test)]
mod datum_test;
#[cfg(any(test, feature = "test"))]
mod dt_fn_arbitrary;
pub mod dt_fn_compare;
#[cfg(test)]
mod dt_fn_compare_test;

pub mod dt_fn_convert;
pub mod dt_fn_param;
#[cfg(test)]
mod dt_fn_param_test;
mod dt_impl;
pub mod dt_info;
mod dt_kind;
pub mod dt_param;
pub mod len_kind;
#[cfg(test)]
mod len_kind_test;

pub mod param;

mod dat_json;
pub mod dat_msg_pack;
#[cfg(test)]
mod dat_msg_pack_test;

pub mod dat_value_inner;
pub mod dt_function;
#[cfg(test)]
mod dt_function_test;

pub mod dtp_array;
#[cfg(test)]
mod dtp_array_test;

pub mod dtp_kind;
#[cfg(test)]
mod dtp_kind_test;

pub mod dtp_numeric;
pub mod dtp_object;
#[cfg(test)]
mod dtp_object_test;

pub mod dtp_string;
#[cfg(test)]
mod dtp_string_test;

pub mod dtp_time;
#[cfg(test)]
mod dtp_time_test;

pub mod dtp_timestamp;
#[cfg(test)]
mod dtp_timestamp_test;

pub mod dtp_timestamptz;
#[cfg(test)]
mod dtp_timestamptz_test;

pub mod record;
pub mod scalar_type;

#[cfg(test)]
mod scalar_type_test;

pub mod string;
#[cfg(test)]
mod string_test;

pub mod type_error;
#[cfg(test)]
mod type_error_test;
//pub mod universal;
