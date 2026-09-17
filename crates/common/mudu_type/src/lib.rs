pub mod array;
pub mod data_binary;
#[cfg(test)]
mod data_binary_test;
pub mod data_textual;
pub mod data_type;
#[cfg(any(test, feature = "test"))]
mod data_type_fn_arbitrary;
pub mod data_type_fn_compare;
#[cfg(test)]
mod data_type_fn_compare_test;
pub mod data_typed;
#[cfg(test)]
mod data_typed_test;
pub mod data_value;
#[cfg(test)]
mod data_value_array_test;
pub mod datum;
#[cfg(test)]
mod datum_test;
pub mod type_family;

pub mod data_type_fn_convert;
pub mod data_type_fn_param;
#[cfg(test)]
mod data_type_fn_param_test;
mod data_type_impl;
pub mod data_type_info;
pub mod data_type_param;
pub mod len_kind;
#[cfg(test)]
mod len_kind_test;
mod type_kind;

pub mod param;

mod data_json;
pub mod data_msg_pack;
#[cfg(test)]
mod data_msg_pack_test;

pub mod data_type_function;
#[cfg(test)]
mod data_type_function_test;
pub mod data_value_inner;

pub mod data_type_param_array;
#[cfg(test)]
mod data_type_param_array_test;

pub mod data_type_param_kind;
#[cfg(test)]
mod data_type_param_kind_test;

pub mod data_type_param_numeric;
pub mod data_type_param_record;
#[cfg(test)]
mod data_type_param_record_test;

pub mod data_type_param_string;
#[cfg(test)]
mod data_type_param_string_test;

pub mod data_type_param_time;
#[cfg(test)]
mod data_type_param_time_test;

pub mod data_type_param_timestamp;
#[cfg(test)]
mod data_type_param_timestamp_test;

pub mod data_type_param_timestamptz;
#[cfg(test)]
mod data_type_param_timestamptz_test;

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
